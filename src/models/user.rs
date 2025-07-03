use serde::{Deserialize, Serialize};
use mongodb::bson::{Document, oid::ObjectId};

fn default_user_type() -> String {
    "customer".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Preferences(pub Document);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub wallet_address: String,
    pub username: String,
    pub preferences: Preferences,
    #[serde(default)]  // Will default to false for old records
    pub is_verified: bool,
    #[serde(default = "default_user_type")]  // Will default to "customer" for old records
    pub user_type: String, 
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub wallet_address: String,
    pub username: String,
    pub preferences: Option<Preferences>,
    pub is_verified: bool,
    pub user_type: String,  // "customer" or "vendor"
    // Optional vendor fields (only used if user_type = "vendor")
    pub vendor_description: Option<String>,
    pub vendor_google_maps_link: Option<String>,
    pub vendor_website_link: Option<String>,
}