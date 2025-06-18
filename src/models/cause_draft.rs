use serde::{Deserialize, Serialize};
use mongodb::bson::{self, oid::ObjectId};
use chrono::{DateTime, Utc, Duration};

mod option_datetime_as_bson {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use chrono::{DateTime, Utc};
    use mongodb::bson;

    pub fn serialize<S>(
        date: &Option<DateTime<Utc>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match date {
            Some(dt) => bson::DateTime::from_chrono(*dt).serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt: Option<bson::DateTime> = Option::deserialize(deserializer)?;
        Ok(opt.map(|dt| dt.to_chrono()))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum DraftStatus {
    #[serde(rename = "draft")]
    Draft,
    #[serde(rename = "stripe_pending")]
    StripePending,
    #[serde(rename = "processing")]
    Processing,
    #[serde(rename = "completed")]
    Completed,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CauseDraft {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub organization: String,
    pub description: String,
    pub long_description: String,
    pub creator_email: String,
    pub token_name: String,
    pub token_symbol: String,
    pub token_image_url: Option<String>,
    pub cause_image_url: Option<String>,
    pub stripe_account_id: Option<String>,
    pub status: DraftStatus,
    pub cause_id: Option<String>, // ID of the created cause if completed
    #[serde(skip_serializing_if = "Option::is_none", with = "option_datetime_as_bson", default)]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "bson::serde_helpers::chrono_datetime_as_bson_datetime")]
    pub expires_at: DateTime<Utc>,
}

impl CauseDraft {
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
        let now = Utc::now();
        Self {
            id: None,
            name,
            organization,
            description,
            long_description,
            creator_email,
            token_name,
            token_symbol,
            token_image_url,
            cause_image_url,
            stripe_account_id: None,
            status: DraftStatus::Draft,
            cause_id: None,
            completed_at: None,
            created_at: now,
            expires_at: now + Duration::days(1), // Auto-expire after 1 day for incomplete drafts
        }
    }
}