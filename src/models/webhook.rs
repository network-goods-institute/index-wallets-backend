use thiserror::Error;
use stripe;

#[derive(Error, Debug)]
pub enum WebhookError {
    #[error("Stripe error: {0}")]
    StripeError(#[from] stripe::WebhookError),
    
    #[error("Invalid payload: {0}")]
    InvalidPayload(String),
    
    #[error("Missing Stripe signature")]
    MissingSignature,
    
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),
    
    #[error("Invalid public key: {0}")]
    InvalidPublicKey(String),
    
    #[error("Token transfer failed: {0}")]
    TokenTransferError(String),
}
