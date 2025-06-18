use serde::{Deserialize, Serialize};
use mongodb::bson::{Document, oid::ObjectId};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Preferences(pub Document);

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub wallet_address: String,
    pub username: String,
    pub preferences: Preferences, 
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub wallet_address: String,
    pub username: String,
    pub preferences: Option<Preferences>,
}