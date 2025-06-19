use serde::{Deserialize, Serialize};
use mongodb::bson::Document;
use crate::models::{TokenBalance, TokenPayment, DiscountConsumption, TokenValuation};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Payment {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<mongodb::bson::oid::ObjectId>,
    pub payment_id: String,
    pub vendor_address: String,
    pub vendor_name: String,
    pub price_usd: f64,
    pub customer_address: Option<String>,
    pub customer_username: Option<String>,
    pub status: PaymentStatus,
    pub created_at: i64,
    pub vendor_valuations: Option<Vec<TokenValuation>>,
    pub discount_consumption: Option<Vec<DiscountConsumption>>,
    pub computed_payment: Option<Vec<TokenPayment>>,
    pub initial_payment_bundle: Option<Vec<TokenPayment>>,  // Before discounts
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreatePaymentRequest {
    pub vendor_address: String,
    pub vendor_name: String,
    pub price_usd: f64,
    pub vendor_valuations: Option<Vec<TokenValuation>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PaymentIdResponse {
    pub payment_id: String,
    pub vendor_name: String,
    pub price_usd: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SupplementPaymentRequest {
    pub payer_address: String,
    pub payer_username: Option<String>,
    pub payer_balances: Vec<TokenBalance>
}


#[derive(Serialize, Deserialize, Debug)]
pub struct SupplementPaymentResponse {
    pub payment_id: String,
    pub vendor_address: String,
    pub vendor_name: String,
    pub customer_address: Option<String>,
    pub status: PaymentStatus,
    pub price_usd: f64,
    pub created_at: i64,
    pub payment_bundle: Vec<TokenPayment>,
    pub unsigned_transaction: String,
    pub vendor_valuations: Option<Vec<TokenValuation>>,
    pub discount_consumption: Option<Vec<DiscountConsumption>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessSignedTransactionRequest {
    pub payment_id: String,
    pub signed_transaction: String,
    pub vendor_address: String,
    pub vendor_name: String,
    pub price_usd: f64,
    pub payment_bundle: Vec<TokenPayment>,
    pub payer_address: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PaymentStatusResponse {
    pub payment_id: String,
    pub vendor_address: String,
    pub vendor_name: String,
    pub customer_address: Option<String>,
    pub status: PaymentStatus,
    pub price_usd: f64,
    pub created_at: i64,
    pub payment_bundle: Option<Vec<TokenPayment>>,
    pub computed_payment: Option<Vec<TokenPayment>>,
    pub vendor_valuations: Option<Vec<TokenValuation>>,
    pub discount_consumption: Option<Vec<DiscountConsumption>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum PaymentStatus {
    Created,
    CustomerAssigned,
    Calculated,
    Completed,
    Failed,
}

impl std::fmt::Display for PaymentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            PaymentStatus::Created => write!(f, "Created"),
            PaymentStatus::CustomerAssigned => write!(f, "CustomerAssigned"),
            PaymentStatus::Calculated => write!(f, "Calculated"),
            PaymentStatus::Completed => write!(f, "Completed"),
            PaymentStatus::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TransactionDirection {
    Sent,     // User was the customer (customer_address)
    Received, // User was the vendor (vendor_address)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TransactionHistoryItem {
    pub payment_id: String,
    pub direction: TransactionDirection,
    pub counterparty_address: String,
    pub counterparty_username: Option<String>,
    pub vendor_name: String,
    pub status: PaymentStatus,
    pub price_usd: f64,
    pub created_at: i64,
    pub computed_payment: Option<Vec<TokenPayment>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionHistoryResponse {
    pub activities: Vec<ActivityItem>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ActivityItem {
    #[serde(rename = "transaction")]
    Transaction(TransactionHistoryItem),
    #[serde(rename = "deposit")]
    Deposit(DepositRecord),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DepositRecord {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<mongodb::bson::oid::ObjectId>,
    pub wallet_address: String,
    pub token_symbol: String,
    pub token_image_url: Option<String>,
    pub amount_deposited_usd: f64,
    pub amount_tokens_received: f64,
    pub created_at: i64, // Unix timestamp to match transactions
}
