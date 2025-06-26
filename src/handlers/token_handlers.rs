use actix_web::{web, HttpResponse};
use log::{info, error};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::services::TokenService;
use delta_executor_sdk::base::crypto::Ed25519PrivKey;
use crate::models::ApiError;

#[derive(Deserialize)]
pub struct CreateTokenRequest {
    pub name: String,
    pub symbol: String,
    pub initial_supply: u64,
    pub image_url: Option<String>,
}

#[derive(Serialize)]
pub struct CreateTokenResponse {
    pub token_id: String,
    pub token_name: String,
    pub token_symbol: String,
    pub initial_supply: u64,
    pub issuer_pubkey: String,
    pub image_url: Option<String>,
}

/// Create a new token with initial supply
pub async fn create_token(
    token_service: web::Data<TokenService>,
    token_data: web::Json<CreateTokenRequest>,
) -> HttpResponse {
    info!("Creating new token: {} ({})", token_data.name, token_data.symbol);
    
    // Generate a new keypair for the token issuer
    let issuer_keypair = Ed25519PrivKey::generate();
    let issuer_pubkey = issuer_keypair.pub_key();
    
    info!("Generated issuer keypair with pubkey: {}", issuer_pubkey);
    
    // Create the token using the same logic as USD token
    match token_service.create_token(
        &issuer_keypair,
        &token_data.name,
        &token_data.symbol,
        token_data.initial_supply,
        token_data.image_url.clone(),
    ).await {
        Ok(token) => {
            info!("Successfully created token {} with ID: {}", token.token_name, token.token_id);
            
            let response = CreateTokenResponse {
                token_id: token.token_id,
                token_name: token.token_name,
                token_symbol: token.token_symbol.unwrap_or_default(),
                initial_supply: token_data.initial_supply,
                issuer_pubkey: issuer_pubkey.to_string(),
                image_url: token.token_image_url,
            };
            
            HttpResponse::Created().json(response)
        },
        Err(e) => {
            error!("Failed to create token: {}", e);
            HttpResponse::InternalServerError().json(json!({
                "error": "Failed to create token",
                "details": e.to_string()
            }))
        }
    }
}