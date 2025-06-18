use actix_web::web;
use log::{info, error};
use serde::Serialize;
use serde_json::Value;
use std::str::FromStr;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use delta_executor_sdk::{
    self,
    base::{
        core::Planck,
        crypto::{Ed25519PubKey, Ed25519PrivKey, SignedMessage, HashDigest},
        vaults::{VaultId, Vault, TokenKind, ReadableVault},
        verifiable::VerifiableType,
    },
    runtime::Error as RuntimeError,
};
use crate::services::executor_client::ExecutorClient;
use crate::services::MongoDBService;
use crate::models::Token;


#[derive(Debug, Serialize)]
pub struct WalletToken {
    pub token_id: String,
    pub token_name: String,
    pub balance: u64,
}

#[derive(Debug)]
pub enum WalletError {
    InvalidPublicKeyFormat(String),
    InvalidPublicKeyLength(usize),
    RuntimeError(String),
    HexDecodeError(hex::FromHexError),
}

impl fmt::Display for WalletError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WalletError::InvalidPublicKeyFormat(msg) => write!(f, "Invalid public key format: {}", msg),
            WalletError::InvalidPublicKeyLength(len) => write!(f, "Invalid public key length: {} bytes (expected 32)", len),
            WalletError::RuntimeError(msg) => write!(f, "Runtime error: {}", msg),
            WalletError::HexDecodeError(e) => write!(f, "Hex decode error: {}", e),
        }
    }
}

impl std::error::Error for WalletError {}

impl From<hex::FromHexError> for WalletError {
    fn from(err: hex::FromHexError) -> Self {
        WalletError::HexDecodeError(err)
    }
}

impl From<RuntimeError> for WalletError {
    fn from(err: RuntimeError) -> Self {
        WalletError::RuntimeError(format!("{:?}", err))
    }
}

#[derive(Debug, Serialize)]
pub struct TokenMetadataInfo {
    name: String,
    symbol: String,
    market_valuation: f64,
    total_allocated: u64,
    token_image_url: String, 
}

impl Default for TokenMetadataInfo {
    fn default() -> Self {
        Self {
            name: "Unknown".to_string(),
            symbol: "???".to_string(),
            market_valuation: 1.0,
            total_allocated: 0,
            token_image_url: "".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TokenInfo {
    balance: u64,
    #[serde(flatten)]
    metadata: TokenMetadataInfo,
}

pub struct WalletService {
    executor_client: ExecutorClient,
    mongodb: web::Data<MongoDBService>,
}

impl WalletService {
    pub fn new(mongodb: web::Data<MongoDBService>) -> Self {
        Self { 
            executor_client: ExecutorClient::new(),
            mongodb,
        }
    }
    
    /// Get a vault by public key
    pub async fn get_vault(&self, pubkey: &Ed25519PubKey) -> Result<Option<Vault>, WalletError> {
        self.executor_client
            .get_vault(pubkey)
            .await
            .map_err(|e| WalletError::RuntimeError(e))
    }


    // pub async fn get_wallet_tokens(&self, pubkey: &Ed25519PubKey) -> Result<Vec<WalletToken>, WalletError> {
    //     // 1. Get the vault
    //     let vault = match self.get_vault(pubkey).await? {
    //         Some(v) => v,
    //         None => return Ok(vec![]),  // Return empty vector if vault doesn't exist
    //     };

    //     println!("Vault: {:?}", vault);

    //     // 2. Map the vault tokens
    //     // let token_info = self.map_vault_tokens(&vault).await?;

    //     // // 3. Transform into frontend-friendly format
    //     // Ok(token_info
    //     //     .into_iter()
    //     //     .map(|(token_id, info)| WalletToken {
    //     //         token_id,
    //     //         token_name: info.metadata.name,
    //     //         balance: info.balance,
    //     //     })
    //     //     .collect())
    // }

    
    // here, what we want to do is map the vault token ids (keys) found in get_vault, to the actual token information
    // e.g. token name and symbol

    pub async fn map_vault_tokens(&self, vault: &Vault) -> Result<HashMap<String, TokenInfo>, WalletError> {
        // 1. Prepare token IDs and balances for batch query
        // Get token balances from vault data
        let token_balances = if let Some(data) = vault.data() {
            // Convert VaultDataType to serde_json::Value
            let data_value = serde_json::to_value(data).unwrap_or(Value::Null);
            if let Some(holdings) = data_value.get("TokenHoldings") {
                if let Some(map) = holdings.get("holdings") {
                    if let Some(holdings_obj) = map.as_object() {
                        holdings_obj.iter()
                            .map(|(k, v)| (k.to_string(), v.as_u64().unwrap_or_default()))
                            .collect()
                    } else {
                        HashMap::new()
                    }
                } else {
                    HashMap::new()
                }
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };

        // 2. Batch query MongoDB for token metadata
        let metadata_list = self.mongodb
            .get_tokens_by_ids(&token_balances.keys().cloned().collect::<Vec<_>>())
            .await
            .map_err(|e| WalletError::RuntimeError(format!("Failed to fetch token metadata: {}", e)))?;

        // 3. Create final mapping with both balance and metadata
        Ok(token_balances
            .into_iter()
            .map(|(token_id, balance)| {
                let metadata = metadata_list
                    .iter()
                    .find(|m| m.token_id == token_id)
                    .map(|m| TokenMetadataInfo {
                        name: m.token_name.clone(),
                        token_image_url: m.token_image_url.clone().unwrap_or_default(),
                        symbol: m.token_symbol.clone().unwrap_or_default(),
                        total_allocated: m.total_allocated,
                        market_valuation: m.market_valuation,
                    })
                    .unwrap_or_default();

                (token_id, TokenInfo { balance, metadata })
            })
            .collect())
    }

    /// Submit verifiable messages to the executor
    pub async fn submit_verifiables(&self, verifiables: Vec<VerifiableType>) -> Result<(), WalletError> {
        self.executor_client
            .submit_verifiables(verifiables)
            .await
            .map_err(|e| WalletError::RuntimeError(e))
    }
    
    /// Parse a public key from a string (supports both Base58 and hex formats)
    pub fn parse_public_key(key_str: &str) -> Result<Ed25519PubKey, WalletError> {
        // Try Base58 first
        match Ed25519PubKey::from_str(key_str) {
            Ok(pk) => {
                info!("Successfully parsed public key using Base58 format");
                Ok(pk)
            },
            Err(e) => {
                // If standard parsing fails, try to handle hexadecimal format
                info!("Base58 parsing failed: {:?}. Trying hex format...", e);
                
                // Check if it looks like a hex string (remove 0x prefix if present)
                let hex_str = if key_str.starts_with("0x") {
                    &key_str[2..]
                } else {
                    key_str
                };
                
                // Try to parse as hex
                let bytes = hex::decode(hex_str)?;
                
                // Ensure we have the right number of bytes for a public key
                if bytes.len() != 32 {
                    return Err(WalletError::InvalidPublicKeyLength(bytes.len()));
                }
                
                // Convert bytes to PubKey
                match Ed25519PubKey::try_from(bytes.as_slice()) {
                    Ok(pk) => {
                        info!("Successfully parsed public key from hex");
                        Ok(pk)
                    },
                    Err(e) => {
                        error!("Failed to convert hex bytes to PubKey: {:?}", e);
                        Err(WalletError::InvalidPublicKeyFormat(format!("Failed to convert hex to PubKey: {:?}", e)))
                    }
                }
            }
        }
    }
    
    
}
