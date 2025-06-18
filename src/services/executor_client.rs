use reqwest::{Client, StatusCode};
use log::{info, error};
use delta_executor_sdk::base::{
    crypto::{HashDigest, Ed25519PubKey},
    vaults::Vault,
    verifiable::VerifiableType,
};
use std::env;
use serde_json;

/// Client for communicating with the Delta Executor service
#[derive(Clone)]
pub struct ExecutorClient {
    base_url: String,
    client: Client,
}

impl ExecutorClient {
    /// Create a new ExecutorClient
    pub fn new() -> Self {
        let host = env::var("SERVER_HOST").unwrap_or_else(|_| "localhost".to_string());
        let port = env::var("EXECUTOR_PORT")
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(8081);
        
        let base_url = format!("http://{}:{}", host, port);
        info!("Executor client connecting to: {}", base_url);
        
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }
    
    /// Get a vault by public key
    pub async fn get_vault(&self, pubkey: &Ed25519PubKey) -> Result<Option<Vault>, String> {
        info!("Requesting vault for public key: {}", pubkey);
        
        let url = format!("{}/vaults/{}", self.base_url, pubkey);
        
        match self.client.get(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    match response.json::<Vault>().await {
                        Ok(vault) => {
                            info!("Successfully retrieved vault");
                            Ok(Some(vault))
                        },
                        Err(e) => {
                            error!("Failed to deserialize vault: {:?}", e);
                            Err(format!("Failed to deserialize vault: {:?}", e))
                        }
                    }
                } else if response.status() == StatusCode::NOT_FOUND {
                    info!("Vault not found for public key: {}", pubkey);
                    Ok(None)
                } else {
                    let error = format!("Failed to get vault: HTTP {}", response.status());
                    error!("{}", error);
                    Err(error)
                }
            },
            Err(e) => {
                error!("Request to executor service failed: {:?}", e);
                Err(format!("Request to executor service failed: {:?}", e))
            }
        }
    }
    
    /// Submit verifiable messages to the executor
    pub async fn submit_verifiables(&self, verifiables: Vec<VerifiableType>) -> Result<(), String> {
        let url = format!("{}/execute", self.base_url);
        info!("Attempting to submit {} verifiables to URL: {}", verifiables.len(), url);

        match self.client.post(&url)
            .json(&verifiables)
            .send()
            .await 
        {
            Ok(response) => {
                if response.status().is_success() {
                    info!("Successfully submitted {} verifiables", verifiables.len());
                    Ok(())
                } else {
                    let status = response.status();
                    let error_body = response.text().await.unwrap_or_else(|_| "unable to read error response".to_string());
                    let error = format!("Failed to submit verifiables: HTTP {} - {}", status, error_body);
                    error!("{}", error);
                    Err(error)
                }
            },
            Err(e) => {
                error!("Request to executor service failed: {:?}", e);
                Err(format!("Request to executor service failed: {:?}", e))
            }
        }
    }
}
