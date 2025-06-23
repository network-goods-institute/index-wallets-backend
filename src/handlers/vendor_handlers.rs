use actix_web::{web, HttpResponse};
use log::{info, error};
use serde_json::json;
use crate::services::MongoDBService;

/// Get all partnered vendors
pub async fn get_partnered_vendors(mongodb: web::Data<MongoDBService>) -> HttpResponse {
    info!("Fetching all partnered vendors");
    
    match mongodb.get_all_partnered_vendors().await {
        Ok(vendors) => {
            info!("Found {} partnered vendors", vendors.len());
            HttpResponse::Ok().json(vendors)
        },
        Err(e) => {
            error!("Error fetching partnered vendors: {}", e);
            HttpResponse::InternalServerError().json(json!({
                "error": "Failed to fetch partnered vendors",
                "details": e.to_string()
            }))
        }
    }
}