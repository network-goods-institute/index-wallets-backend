#![allow(unused_imports, unused_variables)]
use std::collections::{HashMap, BTreeMap};
use std::sync::Arc;
use actix_web::{
    App, 
    HttpServer, 
    web, 
    HttpRequest,
    HttpResponse, 
    Responder,
    error::{ErrorInternalServerError, ErrorBadRequest},
    middleware::DefaultHeaders
};
use actix_cors::Cors;
use actix_web::web::Bytes;
use log::{info, error};
use dotenv::dotenv;
use std::{env, path::PathBuf, sync::Mutex, str::FromStr, time::{SystemTime, UNIX_EPOCH}, fs};
use delta_executor_sdk::base::crypto::{Ed25519PrivKey, Ed25519PubKey, read_keypair};
use delta_executor_sdk::base::core::Shard;
use delta_executor_sdk::base::vaults::{VaultId, TokenKind, Vault, ReadableVault};
use delta_executor_sdk::base::verifiable::{debit_allowance::{DebitAllowance, SignedDebitAllowance}, VerifiableType};
use serde::{Deserialize, Serialize};
use serde_json::json;
mod models;
mod handlers;
mod routes;
mod services;
mod utils;
mod config;
use services::{MongoDBService, TokenService, WalletService, CauseService, WebhookService};
use config::KeyConfig;
use stripe::Client;

#[derive(Debug, Serialize, Deserialize)]
struct SignedTransaction {
    signed_debit_allowance: SignedDebitAllowance,
}


async fn initialize_usd_token(token_service: &TokenService) -> Result<(), Box<dyn std::error::Error>> {
    info!("Checking if USD token exists...");
    
    // Check if USD token already exists in the database
    match token_service.get_token_by_name("USD").await {
        Ok(Some(token)) => {
            info!("USD token already exists with ID: {}", token.token_id);
            Ok(())
        },
        Ok(None) => {
            info!("USD token not found, creating new USD token...");
            
            // Generate a new keypair for the USD token issuer
            let issuer_keypair = Ed25519PrivKey::generate();
            info!("Generated USD issuer keypair with pubkey: {}", issuer_keypair.pub_key());
            
            // Create the USD token with initial supply
            match token_service.create_token(
                &issuer_keypair,
                "USD",
                "USD",
                1000000000, // 1 billion initial supply
                Some("https://cdn.midjourney.com/487ad972-7260-4dfd-a8e8-8d3ea0911a90/0_2.png".to_string()), // Add your USD logo URL here
            ).await {
                Ok(token) => {
                    info!("Successfully created USD token with ID: {}", token.token_id);
                    Ok(())
                },
                Err(e) => {
                    error!("Failed to create USD token: {}", e);
                    Err(format!("Failed to create USD token: {}", e).into())
                }
            }
        },
        Err(e) => {
            error!("Failed to check for existing USD token: {}", e);
            Err(format!("Failed to check for existing USD token: {}", e).into())
        }
    }
}

#[actix_web::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load .env file
    dotenv().ok();
    
    // Get environment variables with defaults
    let host = env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = env::var("SERVER_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .expect("SERVER_PORT must be a number");
    let log_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let base_rpc = env::var("BASE_RPC").unwrap_or_else(|_| delta_executor_sdk::base::rpc::DEFAULT_URL.to_string());
    let stripe_api = env::var("STRIPE_SECRET_KEY").unwrap_or_else(|e| {
        error!("STRIPE_SECRET_KEY not found in environment: {}", e);
        "".to_string()
    });
    let stripe_webhook_secret = env::var("STRIPE_WEBHOOK_SECRET").unwrap_or_else(|_| "".to_string());

    env_logger::init_from_env(env_logger::Env::new().default_filter_or(log_level));
    
    // Log Stripe configuration status
    if stripe_api.is_empty() {
        error!("STRIPE_SECRET_KEY is empty - Stripe operations will fail!");
    } else {
        info!("Stripe API key loaded: {} characters, starts with: {}, type: {}", 
            stripe_api.len(),
            &stripe_api.chars().take(7).collect::<String>(),
            if stripe_api.contains("_live_") { "LIVE" } else if stripe_api.contains("_test_") { "TEST" } else { "UNKNOWN" }
        );
    }
    
    let mongodb = MongoDBService::init()
        .await
        .expect("Failed to initialize MongoDB");
    let mongodb_data = web::Data::new(mongodb);
    
    // Load keypairs from environment variables or JSON files
    let key_config = KeyConfig::load()
        .expect("Failed to load keypair configuration");
    
    info!("Central vault pubkey: {}", key_config.central_vault_pubkey);
    info!("Network goods vault pubkey: {}", key_config.network_goods_vault_pubkey);

    let wallet_service = web::Data::new(WalletService::new(mongodb_data.clone()));
    
    let token_service = web::Data::new(TokenService::new(
        mongodb_data.clone(),
        key_config.central_vault_keypair.clone()
    ));
    
    initialize_usd_token(&token_service).await?;
    
    let stripe_client = stripe::Client::new(&stripe_api);
    let stripe_client_arc = Arc::new(stripe_client.clone());
    let stripe_client_data = web::Data::new(stripe_client);

    let cause_service = web::Data::new(CauseService::new(
        Arc::new(mongodb_data.get_ref().clone()),
        Arc::new(token_service.get_ref().clone()),
        stripe_client_arc.clone()
    ));

    let webhook_service = web::Data::new(WebhookService::new(
        stripe_webhook_secret,
        Arc::new(token_service.get_ref().clone()),
        Arc::new(mongodb_data.get_ref().clone()),
        key_config.central_vault_keypair.clone(),
        key_config.network_goods_vault_keypair.clone()
    ));
    
    info!("Starting server at http://{}:{}", host, port);
    
    HttpServer::new(move || {
        // Configure CORS middleware
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .expose_headers(vec!["content-type", "content-length", "accept"])
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(mongodb_data.clone())
            .app_data(wallet_service.clone())
            .app_data(token_service.clone())
            .app_data(cause_service.clone())
            .app_data(stripe_client_data.clone())
            .app_data(webhook_service.clone())
            .configure(routes::configure)
            .route("/submit-signed-transaction", web::post().to(receive_signed))
            .route("/health", web::get().to(|| async {
                info!("Health check");
                HttpResponse::Ok().body("OK")
            }))
            .route("/receive-signed", web::post().to(receive_signed))
    })
    .bind(format!("{host}:{port}"))?
    .run()
    .await?;

    info!("Server shutting down");
    Ok(())
}


async fn receive_signed(wallet_service: web::Data<WalletService>, payload: web::Json<SignedTransaction>) -> HttpResponse {
    info!("Received signed debit allowance");
    
    // Submit the signed transaction
    match wallet_service.submit_verifiables(vec![VerifiableType::DebitAllowance(payload.signed_debit_allowance.clone())]).await {
        Ok(_) => {
            info!("Successfully submitted debit allowance");
            HttpResponse::Ok().json(json!({
                "status": "success",
                "message": "Debit allowance submitted successfully"
            }))
        },
        Err(e) => {
            error!("Failed to submit debit allowance: {}", e);
            HttpResponse::InternalServerError().json(json!({
                "error": "Failed to submit debit allowance",
                "details": e.to_string()
            }))
        }
    }
}


