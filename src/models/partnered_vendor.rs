use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PartneredVendor {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub wallet_address: String,
    pub description: Option<String>,
    pub google_maps_link: Option<String>,
    pub website_link: Option<String>,
}

impl PartneredVendor {
    pub fn new(
        name: String,
        wallet_address: String,
        description: Option<String>,
        google_maps_link: Option<String>,
        website_link: Option<String>,
    ) -> Self {
        Self {
            id: None,
            name,
            wallet_address,
            description,
            google_maps_link,
            website_link,
        }
    }
}