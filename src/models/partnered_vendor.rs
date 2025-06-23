use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PartneredVendor {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub description: String,
    pub google_maps_link: String,
    pub website_link: String,
    pub image_url: String,
}

impl PartneredVendor {
    pub fn new(
        name: String,
        description: String,
        google_maps_link: String,
        website_link: String,
        image_url: String,
    ) -> Self {
        Self {
            id: None,
            name,
            description,
            google_maps_link,
            website_link,
            image_url,
        }
    }
}