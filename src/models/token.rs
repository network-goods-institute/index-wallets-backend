use serde::{Deserialize, Serialize};
use mongodb::bson::Document;
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Token {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<mongodb::bson::oid::ObjectId>,
    // token id is pk of vault + shard 
    pub token_id: String,
    pub token_name: String,
    #[serde(default)]  // This will default to None if field is missing
    pub token_symbol: Option<String>,
    #[serde(default = "default_market_valuation")]
    pub market_valuation: f64, 
    pub total_allocated: u64,
    pub created_at: i64,
    pub stripe_product_id: String,
    pub token_image_url: Option<String>,
}

fn default_market_valuation() -> f64 {
    1.0
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TokenValuation {
    pub token_key: String, 
    pub symbol: String,
    pub valuation: f64, 
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenValuationsResponse {
    pub valuations: Vec<TokenValuation>
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateValuationRequest {
    pub symbol: String,  // Changed from token_name to token_symbol
    pub valuation: f64
}



#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscountConsumption {
    pub token_key: String,
    pub symbol: String,
    pub amount_used: f64,  // how much discount/premium was consumed
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenPayment {
    pub token_key: String,
    pub symbol: String,
    pub amount_to_pay: f64, // units of this token to pay
    #[serde(default)]
    pub token_image_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenBalance {
    pub token_key: String,     // "address,chainId" from frontend
    pub symbol: String,
    pub name: String,
    pub balance: f64,
    // TODO: rename average_valuation to market_valuation
    pub average_valuation: f64,
    #[serde(default)]
    pub token_image_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TransactionRecord {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<mongodb::bson::oid::ObjectId>,
    pub token_key: String,           // "39S38zsewu64uQ96gXJ4Z8MABSzS8HdfCBXJoergmLQo,1"
    pub symbol: String,              // "USD" 
    pub amount_paid: f64,            // 8.0
    pub effective_valuation: f64,    // from vendor_valuations (e.g., 1.0 for USD, 0.8 for discounted GAY)
    #[serde(with = "mongodb::bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub timestamp: DateTime<Utc>,    // current time with BSON serialization
    pub payment_id: String,          // "SA0V"
}