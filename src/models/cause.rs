use serde::{Deserialize, Serialize};
use mongodb::bson::{self, oid::ObjectId};
use chrono::{DateTime, Utc};

fn default_displayed() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum CauseStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "stripe_created")]
    StripeCreated,
    #[serde(rename = "token_minted")]
    TokenMinted,
    #[serde(rename = "active")]
    Active,
    #[serde(rename = "failed")]
    Failed,
}

impl std::fmt::Display for CauseStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CauseStatus::Pending => write!(f, "pending"),
            CauseStatus::StripeCreated => write!(f, "stripe_created"),
            CauseStatus::TokenMinted => write!(f, "token_minted"),
            CauseStatus::Active => write!(f, "active"),
            CauseStatus::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Cause {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub organization: String,
    pub description: String,
    pub long_description: String,
    pub creator_email: String,
    pub token_name: String,
    pub token_symbol: String,
    pub total_raised: u64,
    pub amount_donated: f64,
    pub tokens_purchased: f64,
    pub current_price: f64,
    pub status: CauseStatus,
    pub stripe_product_id: Option<String>,
    pub payment_link: Option<String>,
    pub token_id: Option<String>,
    pub error_message: Option<String>,
    pub is_active: bool,
    pub token_image_url: Option<String>,
    pub cause_image_url: Option<String>,
    pub stripe_account_id: Option<String>,
    pub stripe_account_status: Option<String>,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default)]
    pub payouts_enabled: bool,
    #[serde(default = "default_displayed")]
    pub displayed: bool,
    #[serde(default)]
    pub featured: bool,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl Cause {
    pub fn new(
        name: String,
        organization: String,
        description: String,
        long_description: String,
        creator_email: String,
        token_name: String,
        token_symbol: String,
        token_image_url: Option<String>,
        cause_image_url: Option<String>,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: None,
            name,
            organization,
            description,
            long_description,
            creator_email,
            token_name,
            token_symbol,
            total_raised: 0,
            amount_donated: 0.0,
            tokens_purchased: 0.0,
            current_price: 0.01,  // Initial price: $0.01 per token (1 cent)
            status: CauseStatus::Pending,
            stripe_product_id: None,
            payment_link: None,
            token_id: None,
            error_message: None,
            is_active: true,
            token_image_url,
            cause_image_url,
            stripe_account_id: None,
            stripe_account_status: None,
            onboarding_completed: false,
            payouts_enabled: false,
            displayed: true,
            featured: false,
            created_at: now,
            updated_at: now,
        }
    }
}
