use actix_web::web;
use log::{info, error};
use std::time::{SystemTime, UNIX_EPOCH};
use std::str::FromStr;
use std::env;
use delta_executor_sdk::{
    base::{
        core::Shard,
        crypto::{Ed25519PubKey, Ed25519PrivKey, SignedMessage},
        vaults::{VaultId, TokenMetadata, TokenKind, ReadableVault},
        verifiable::{
            token_mint::{TokenMint, TokenSupplyOperation},
            VerifiableType,
        },
    },
};

use crate::{models::Token, services::{MongoDBService, executor_client::ExecutorClient}};


#[derive(Clone)]
pub struct TokenService {
    mongodb: web::Data<MongoDBService>,
    central_vault_id: VaultId,
    executor_client: ExecutorClient,
}

impl TokenService {
    pub fn new(mongodb: web::Data<MongoDBService>, central_vault_keypair: Ed25519PrivKey) -> Self {
        // Use shard 1 as default for the central vault
        let central_vault_id = VaultId::new(central_vault_keypair.pub_key(), Shard::from(1u64));
        
        Self { 
            mongodb,
            central_vault_id,
            executor_client: ExecutorClient::new()
        }
    }
    
    /// Create a new token and mint the initial supply
    pub async fn create_token_for_cause(
        &self,
        token_name: &str,
        token_symbol: &str,
        initial_supply: u64,
        token_image_url: Option<String>,
    ) -> Result<Token, String> {
        info!("Creating new token for cause: {}", token_name);
        
        // Generate a new keypair for this specific token
        let issuer_keypair = Ed25519PrivKey::generate();
        info!("Generated new issuer keypair with public key: {}", issuer_keypair.pub_key());
        
        self.create_token(
            &issuer_keypair,
            token_name,
            token_symbol,
            initial_supply,
            token_image_url
        ).await
    }

    pub async fn create_token(
        &self,
        issuer_keypair: &Ed25519PrivKey,
        token_name: &str,
        token_symbol: &str,
        initial_supply: u64,
        token_image_url: Option<String>,
    ) -> Result<Token, String> {
        info!("Creating new token: {} with symbol: {}", token_name, token_symbol);
        info!("Initial supply: {}", initial_supply);
        
        // Create token issuer vault ID using same shard as central vault
        
        let issuer_shard = 1;
        let token_issuer = VaultId::new(issuer_keypair.pub_key(), issuer_shard);
        
        // Initial nonce for nonexistent vault
        let new_nonce = 1;
        
        info!("Token issuer vault: {:?}", token_issuer);
        info!("Central vault to be credited: {:?}", self.central_vault_id);
        info!("Using shard: {}", issuer_shard);
        
        
        // Create token metadata
        let metadata = TokenMetadata {
            name: token_name.to_string(),
            symbol: token_symbol.to_string(),
        };
        
        // Create token mint payload
        let payload = TokenMint {
            operation: TokenSupplyOperation::Create {
                metadata,
                credited: vec![(self.central_vault_id, initial_supply)],
            },
            debited: token_issuer,
            new_nonce,
        };
        
        // Create token object for database
        let token = Token {
            id: None,
            // Use the pubkey and shard from the token_issuer VaultId with comma separator
            token_id: format!("{},{}", token_issuer.pubkey(), token_issuer.shard()),
            token_name: token_name.to_string(),
            token_symbol: Some(token_symbol.to_string()),
            market_valuation: 1.0,
            total_allocated: initial_supply,
            created_at: SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
            stripe_product_id: "".to_string(),
            token_image_url,
        };
        
        // Sign the payload
        info!("Signing token mint payload");
        let signed = SignedMessage::sign(payload, issuer_keypair)
            .map_err(|e| format!("Failed to sign message: {:?}", e))?;
        
        // Create a verifiable type
        let verifiable = VerifiableType::TokenMint(signed);
        
        // Submit to executor
        match self.executor_client.submit_verifiables(vec![verifiable]).await {
            Ok(_) => {
                info!("Successfully submitted token mint to executor");
                
                // Save token to database
                match self.mongodb.save_token(token.clone()).await {
                    Ok(_) => {
                        info!("Successfully saved token to database");
                        Ok(token)
                    },
                    Err(e) => {
                        error!("Failed to save token to database: {:?}", e);
                        Err(format!("Failed to save token to database: {:?}", e))
                    }
                }
            },
            Err(e) => {
                error!("Failed to submit token mint to executor: {}", e);
                Err(format!("Failed to submit token mint to executor: {}", e))
            }
        }
    }
    
    /// Get a token by name
    pub async fn get_token_by_name(&self, token_name: &str) -> Result<Option<Token>, String> {
        self.mongodb.get_token_by_name(token_name).await
            .map_err(|e| format!("Failed to get token from database: {:?}", e))
    }

    pub async fn get_token_by_symbol(&self, token_symbol: &str) -> Result<Option<Token>, String> {
        self.mongodb.get_token_by_symbol(token_symbol).await
            .map_err(|e| format!("Failed to get token from database: {:?}", e))
    }

    
    /// Transfer tokens from one vault to another
    pub async fn transfer_tokens(
        &self,
        from_keypair: &Ed25519PrivKey,
        to_pubkey: &Ed25519PubKey,
        token_symbol: &str,
        amount: u64,
    ) -> Result<(), String> {
        // Get token information by symbol
        let token = match self.mongodb.get_token_by_symbol(token_symbol).await
            .map_err(|e| format!("Failed to get token from database: {:?}", e))? {
            Some(token) => token,
            None => return Err(format!("Token not found: {}", token_symbol)),
        };
        
        // Parse token ID
        let token_id_parts: Vec<&str> = token.token_id.split(',').collect();
        if token_id_parts.len() != 2 {
            return Err(format!("Invalid token ID format: {}", token.token_id));
        }
        
        let token_pubkey = Ed25519PubKey::from_str(token_id_parts[0])
            .map_err(|_| format!("Invalid token pubkey: {}", token_id_parts[0]))?;
        
        let token_shard = token_id_parts[1].parse::<u64>()
            .map_err(|_| format!("Invalid token shard: {}", token_id_parts[1]))?;
        
        // Create token vault ID
        let token_vault_id = VaultId::new(token_pubkey, token_shard);
        
        // Get from vault information
        let from_pubkey = from_keypair.pub_key();

        // Get the vault from the executor
        let from_vault = match self.executor_client.get_vault(&from_pubkey).await {
            Ok(Some(vault)) => vault,
            Ok(None) => return Err(format!("Vault not found for pubkey: {}", from_pubkey)),
            Err(e) => return Err(format!("Error fetching vault: {}", e)),
        };
        
        // Create vault IDs
        let from_vault_id = VaultId::new(from_pubkey, from_vault.shard());
        let to_vault_id = VaultId::new(*to_pubkey, from_vault.shard());
        
        // Get current nonce and calculate new nonce
        let current_nonce = from_vault.nonce();
        let new_nonce = current_nonce + 1;
        
        // Create the token kind based on the token_vault_id
        let token_kind = TokenKind::NonNative(token_vault_id);
        
        // Create a map to store the allowances
        let mut allowances = std::collections::BTreeMap::new();
        allowances.insert(token_kind, amount);

        
        // Create the DebitAllowance structure
        let debit = delta_executor_sdk::base::verifiable::debit_allowance::DebitAllowance {
            debited: from_vault_id,
            credited: to_vault_id,
            new_nonce,
            allowances,
        };


        // Sign the DebitAllowance
        let signed = SignedMessage::sign(debit, from_keypair)
            .map_err(|e| format!("Failed to sign DebitAllowance: {:?}", e))?;
        
        // Create a VerifiableType::DebitAllowance with the signed message
        let verifiable = VerifiableType::DebitAllowance(signed);
        
        // Submit to executor
        match self.executor_client.submit_verifiables(vec![verifiable]).await {
            Ok(_) => {
                info!("Successfully transferred {} tokens from {} to {}", 
                      amount, from_pubkey, to_pubkey);
                Ok(())
            },
            Err(e) => {
                error!("Failed to submit transfer to executor: {}", e);
                Err(format!("Failed to submit transfer to executor: {}", e))
            }
        }
    }
}