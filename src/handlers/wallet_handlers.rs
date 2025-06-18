use actix_web::{web, HttpResponse};
use log::{info, error};
use serde_json::json;
use serde::{Serialize, Deserialize};
use crate::services::{WalletService, MongoDBService, TokenService};
use crate::models::token::{TokenValuation, TokenValuationsResponse, UpdateValuationRequest};
use crate::models::error::ApiError;


#[derive(Serialize, Deserialize, Debug)]
pub struct UserTokenResponse {
    pub token_name: String,
    pub token_symbol: String,
    pub current_valuation: f64,
    pub has_set: bool,
    pub token_image_url: String,
}

/// Get user balances
pub async fn get_user_balances(wallet_address: web::Path<String>, wallet_service: web::Data<WalletService>) -> HttpResponse {
    // Parse the public key
    let pubkey = match WalletService::parse_public_key(&wallet_address) {
        Ok(pk) => pk,
        Err(e) => {
            error!("Invalid public key format: {:?}", e);
            return HttpResponse::BadRequest().body(format!("Invalid public key format: {}", e));
        }
    };
    
    // Get the vault
    match wallet_service.get_vault(&pubkey).await {
        Ok(Some(vault)) => {
            info!("Found vault for public key: {}", pubkey);
            match wallet_service.map_vault_tokens(&vault).await {
                Ok(token_info) => HttpResponse::Ok().json(token_info),
                Err(e) => {
                    error!("Error mapping vault tokens: {:?}", e);
                    HttpResponse::InternalServerError().body(format!("Error mapping vault tokens: {}", e))
                }
            }
        },
        Ok(None) => {
            error!("Vault not found for public key: {}", pubkey);
            HttpResponse::NotFound().body(format!("Vault not found for public key: {}", pubkey))
        },
        Err(e) => {
            error!("Error getting vault: {:?}", e);
            HttpResponse::InternalServerError().body(format!("Error getting vault: {}", e))
        }
    }
}

/// Get all tokens with user's valuations
pub async fn get_user_valuations(
    mongodb: web::Data<MongoDBService>,
    wallet_address: web::Path<String>
) -> HttpResponse {
    info!("Fetching token valuations for user: {}", wallet_address);

    // First get all tokens
    let tokens = match mongodb.get_all_tokens().await {
        Ok(tokens) => tokens,
        Err(e) => {
            error!("Failed to fetch tokens: {}", e);
            return HttpResponse::InternalServerError().json(json!({
                "error": "Failed to fetch tokens",
                "details": e.to_string()
            }));
        }
    };

    // Then get user's valuations
    let user = match mongodb.get_user_by_wallet(&wallet_address).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            error!("User not found: {}", wallet_address);
            return HttpResponse::NotFound().json(json!({
                "error": "User not found",
                "details": format!("No user found with wallet address: {}", wallet_address)
            }));
        },
        Err(e) => {
            error!("Failed to fetch user: {}", e);
            return HttpResponse::InternalServerError().json(json!({
                "error": "Failed to fetch user",
                "details": e.to_string()
            }));
        }
    };

    // Convert tokens to TokenValuation
    let valuations: Vec<UserTokenResponse> = tokens.into_iter().map(|token| {
        let token_symbol = token.token_symbol.clone().unwrap_or_default();
        let current_valuation = user.preferences.0.get_f64(&token_symbol).unwrap_or(0.0);
        UserTokenResponse {
            token_name: token.token_name,
            token_symbol: token_symbol.clone(),
            token_image_url: token.token_image_url.clone().unwrap_or_default(),
            current_valuation,
            has_set: user.preferences.0.contains_key(&token_symbol),
        }
    }).collect();

    HttpResponse::Ok().json(valuations)
}

/// Update token valuation for a user
pub async fn update_user_valuation(
    mongodb: web::Data<MongoDBService>,
    wallet_address: web::Path<String>,
    payload: web::Json<UpdateValuationRequest>,
) -> HttpResponse {
    info!("Updating token valuation for user: {}", wallet_address);

    match mongodb.update_user_valuation(&wallet_address, &payload.symbol, payload.valuation).await {
        Ok(_) => {
            info!("Successfully updated valuation for user {} and token {}", wallet_address, payload.symbol);
            HttpResponse::Ok().json(json!({
                "status": "success",
                "message": "Successfully updated valuation"
            }))
        },
        Err(e) => {
            error!("Failed to update valuation: {}", e);
            match e {
                ApiError::NotFound(msg) => {
                    HttpResponse::NotFound().json(json!({
                        "error": "Not found",
                        "details": msg
                    }))
                },
                _ => HttpResponse::InternalServerError().json(json!({
                    "error": "Failed to update valuation",
                    "details": e.to_string()
                }))
            }
        }
    }
}

pub async fn get_vault(wallet_address: web::Path<String>, wallet_service: web::Data<WalletService>) -> HttpResponse {
    // Parse the public key
    let pubkey = match WalletService::parse_public_key(&wallet_address) {
        Ok(pk) => pk,
        Err(e) => {
            error!("Invalid public key format: {:?}", e);
            return HttpResponse::BadRequest().body(format!("Invalid public key format: {}", e));
        }
    };
    
    // Get the vault
    match wallet_service.get_vault(&pubkey).await {
        Ok(Some(vault)) => {
            info!("Found vault for public key: {}", pubkey);
            info!("Vault: {:?}", vault);
            HttpResponse::Ok().json(vault)
        },
        Ok(None) => {
            error!("Vault not found for public key: {}", pubkey);
            HttpResponse::NotFound().body(format!("Vault not found for public key: {}", pubkey))
        },
        Err(e) => {
            error!("Error getting vault: {:?}", e);
            HttpResponse::InternalServerError().body(format!("Error getting vault: {}", e))
        }
    }
}
