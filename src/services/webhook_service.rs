use std::sync::Arc;
use log::{info, error};
use delta_executor_sdk::base::crypto::{Ed25519PubKey, Ed25519PrivKey};
use std::str::FromStr;

use crate::models::WebhookError;
use crate::utils::bonding_curve::BondingCurve;
use super::{TokenService, MongoDBService};
use mongodb::bson::oid::ObjectId;

pub struct WebhookService {
    stripe_secret: String,
    token_service: Arc<TokenService>,
    mongodb_service: Arc<MongoDBService>,
    central_vault_keypair: Ed25519PrivKey,
    network_goods_vault_keypair: Ed25519PrivKey,
}

impl WebhookService {
    pub fn new(
        stripe_secret: String,
        token_service: Arc<TokenService>,
        mongodb_service: Arc<MongoDBService>,
        central_vault_keypair: Ed25519PrivKey,
        network_goods_vault_keypair: Ed25519PrivKey,
    ) -> Self {
        info!("Network goods vault address: {}", network_goods_vault_keypair.pub_key());
        Self {
            stripe_secret,
            token_service,
            mongodb_service,
            central_vault_keypair,
            network_goods_vault_keypair,
        }
    }

    pub fn get_stripe_secret(&self) -> &str {
        &self.stripe_secret
    }

    pub async fn credit_account(
        &self,
        token_symbol: &str,
        amount: i64,
        user_address: &str,
    ) -> Result<f64, WebhookError> {
        info!(
            "Starting credit_account for user: {}, token: {}, amount: {}", 
            user_address, token_symbol, amount
        );
        
        // Convert i64 to u64 safely
        let amount_u64: u64 = if amount >= 0 {
            amount as u64
        } else {
            error!("Amount must be positive");
            return Err(WebhookError::InvalidAmount("Amount must be positive".to_string()));
        };
        
        // Parse the public key
        let user_pubkey = Ed25519PubKey::from_str(user_address)
            .map_err(|e| WebhookError::InvalidPublicKey(e.to_string()))?;

        // Transfer tokens
        self.token_service
            .transfer_tokens(
                &self.central_vault_keypair,
                &user_pubkey,
                token_symbol,
                amount_u64,
            )
            .await
            .map_err(|e| WebhookError::TokenTransferError(e.to_string()))?;

        info!("Successfully credited {} tokens to user {}", amount, user_address);
        Ok(amount_u64 as f64)
    }

    pub async fn credit_account_with_fee_split(
        &self,
        token_symbol: &str,
        total_amount: i64,
        user_address: &str,
    ) -> Result<f64, WebhookError> {
        info!(
            "Starting credit_account_with_fee_split for user: {}, token: {}, total amount: {} units", 
            user_address, token_symbol, total_amount
        );
        
        // Calculate amounts
        let total_amount_u64 = total_amount as u64;
        let platform_cash_fee = (total_amount_u64 as f64 * 0.05).round() as u64; // Platform keeps 5% in cash
        let amount_to_cause = total_amount_u64 - platform_cash_fee; // Cause gets 95% in cash
        
        // Convert cents to dollars for bonding curve calculations
        // Use amount to cause (95% of total) for token calculation
        let amount_in_dollars = amount_to_cause as f64 / 100.0;
        
        // Get current bonding curve state by looking up cause by token symbol
        let (tokens_minted, new_price) = if token_symbol != "USD" && token_symbol != "unknown" {
            match self.mongodb_service.get_cause_by_token_symbol(token_symbol).await {
                Ok(Some(cause)) => {
                    let curve = BondingCurve::new();
                    let tokens = curve.calculate_tokens_for_amount(amount_in_dollars, cause.tokens_purchased);
                    let new_tokens_purchased = cause.tokens_purchased + tokens;
                    let new_price = curve.calculate_price(new_tokens_purchased);
                    
                    // Update cause with new bonding curve values
                    let new_amount_donated = cause.amount_donated + amount_in_dollars;
                    let cause_id = cause.id.as_ref().unwrap().to_hex();
                    self.mongodb_service.update_cause_bonding_curve(
                        &cause_id,
                        new_amount_donated,
                        new_tokens_purchased,
                        new_price,
                    ).await.map_err(|e| WebhookError::TokenTransferError(format!("Failed to update bonding curve: {}", e)))?;
                    
                    
                    (tokens, new_price)
                },
                Ok(None) => {
                    // Cause not found
                    (amount_to_cause as f64, 1.0)
                },
                Err(e) => {
                    // Database error
                    error!("Failed to look up cause for token {}: {}", token_symbol, e);
                    (amount_to_cause as f64, 1.0)
                }
            }
        } else {
            // USD or unknown token, use simple calculation
            (amount_to_cause as f64, 1.0)
        };
        
        // Convert back to integer tokens
        let tokens_minted_u64 = tokens_minted.round() as u64;
        
        // Platform takes 5/95 of tokens (5.26%) which equals $5 worth when $95 of tokens are minted
        let platform_tokens = (tokens_minted_u64 as f64 * (5.0 / 95.0)).round() as u64;
        let user_tokens = tokens_minted_u64 - platform_tokens;
        
        
        // Parse the public key
        let user_pubkey = Ed25519PubKey::from_str(user_address)
            .map_err(|e| WebhookError::InvalidPublicKey(e.to_string()))?;

        // Transfer tokens to user
        self.token_service
            .transfer_tokens(
                &self.central_vault_keypair,
                &user_pubkey,
                token_symbol,
                user_tokens,
            )
            .await
            .map_err(|e| WebhookError::TokenTransferError(e.to_string()))?;

        // Transfer platform fee tokens to network goods vault
        let network_goods_pubkey = self.network_goods_vault_keypair.pub_key();
        self.token_service
            .transfer_tokens(
                &self.central_vault_keypair,
                &network_goods_pubkey,
                token_symbol,
                platform_tokens,
            )
            .await
            .map_err(|e| WebhookError::TokenTransferError(format!("Failed to transfer platform fee: {}", e)))?;
        
        info!(
            "Successfully distributed tokens: {} to user {}, {} to network goods vault",
            user_tokens, user_address, platform_tokens
        );
        
        Ok(user_tokens as f64)
    }
}
