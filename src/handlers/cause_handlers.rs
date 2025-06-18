use actix_web::{web, HttpResponse, Responder, error::ErrorInternalServerError};
use mongodb::bson::oid::ObjectId;
use log::{info, error};

use crate::models::ApiError;
use crate::services::CauseService;

// Re-export the request/response structs from the service
pub use crate::services::cause_service::{CreateCauseRequest, CreateCauseResponse, UpdateCauseRequest};

// Request struct for creating a donation checkout session
#[derive(serde::Deserialize)]
pub struct CreateDonationSessionRequest {
    pub cause_id: String,
    pub amount_cents: i64, // Amount in cents (e.g., 10000 = $100)
    pub user_wallet_address: String,
}

// Response struct for checkout session
#[derive(serde::Serialize)]
pub struct CreateDonationSessionResponse {
    pub checkout_url: String,
    pub session_id: String,
}

// Create a new cause
pub async fn create_cause(
    cause_service: web::Data<CauseService>,
    cause_data: web::Json<CreateCauseRequest>,
) -> actix_web::Result<impl Responder> {
    info!("Creating new cause: {}", cause_data.name);
    
    match cause_service.create_cause(cause_data.into_inner()).await {
        Ok(response) => {
            info!("Successfully created cause draft");
            Ok(HttpResponse::Created().json(response))
        },
        Err(e) => {
            // Convert ApiError to appropriate HTTP response
            match e {
                ApiError::ValidationError(msg) => {
                    Ok(HttpResponse::BadRequest().json(ErrorResponse { 
                        error: "validation_error".to_string(),
                        message: msg,
                    }))
                },
                ApiError::DuplicateError(msg) => {
                    Ok(HttpResponse::Conflict().json(ErrorResponse { 
                        error: "duplicate_error".to_string(),
                        message: msg,
                    }))
                },
                _ => {
                    error!("Failed to create cause: {}", e);
                    Err(ErrorInternalServerError(e.to_string()))
                }
            }
        }
    }
}

// Get a cause by ID
pub async fn get_cause(
    cause_service: web::Data<CauseService>,
    cause_id: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    info!("Getting cause with ID: {}", cause_id);
    
    let object_id = match ObjectId::parse_str(cause_id.as_ref()) {
        Ok(id) => id,
        Err(e) => {
            error!("Invalid cause ID format: {}", e);
            return Ok(HttpResponse::BadRequest().body(format!("Invalid cause ID format: {}", e)));
        }
    };
    
    match cause_service.get_cause_by_id(&object_id).await {
        Ok(cause) => {
            info!("Found cause: {}", cause.name);
            Ok(HttpResponse::Ok().json(cause))
        },
        Err(e) => match e {
            ApiError::NotFound(msg) => {
                info!("{}", msg);
                Ok(HttpResponse::NotFound().body(msg))
            },
            _ => {
                error!("Error retrieving cause: {}", e);
                Err(ErrorInternalServerError(e.to_string()))
            }
        }
    }
}

// Get all causes (only displayed ones)
pub async fn get_all_causes(
    cause_service: web::Data<CauseService>,
) -> actix_web::Result<impl Responder> {
    info!("Getting all displayed causes");
    
    match cause_service.get_all_causes().await {
        Ok(causes) => {
            info!("Retrieved {} displayed causes", causes.len());
            Ok(HttpResponse::Ok().json(causes))
        },
        Err(e) => {
            error!("Failed to retrieve causes: {}", e);
            Err(ErrorInternalServerError(e.to_string()))
        }
    }
}

// Get featured causes
pub async fn get_featured_causes(
    cause_service: web::Data<CauseService>,
) -> actix_web::Result<impl Responder> {
    info!("Getting featured causes");
    
    match cause_service.get_featured_causes().await {
        Ok(causes) => {
            info!("Retrieved {} featured causes", causes.len());
            Ok(HttpResponse::Ok().json(causes))
        },
        Err(e) => {
            error!("Failed to retrieve featured causes: {}", e);
            Err(ErrorInternalServerError(e.to_string()))
        }
    }
}

// Get all causes (admin - unfiltered)
pub async fn get_all_causes_admin(
    cause_service: web::Data<CauseService>,
) -> actix_web::Result<impl Responder> {
    info!("Getting all causes (unfiltered - admin)");
    
    match cause_service.get_all_causes_unfiltered().await {
        Ok(causes) => {
            info!("Retrieved {} total causes", causes.len());
            Ok(HttpResponse::Ok().json(causes))
        },
        Err(e) => {
            error!("Failed to retrieve all causes: {}", e);
            Err(ErrorInternalServerError(e.to_string()))
        }
    }
}

// Update a cause
pub async fn update_cause(
    cause_service: web::Data<CauseService>,
    cause_id: web::Path<String>,
    update_data: web::Json<UpdateCauseRequest>,
) -> actix_web::Result<impl Responder> {
    info!("Updating cause with ID: {}", cause_id);
    
    let object_id = match ObjectId::parse_str(cause_id.as_ref()) {
        Ok(id) => id,
        Err(e) => {
            error!("Invalid cause ID format: {}", e);
            return Ok(HttpResponse::BadRequest().body(format!("Invalid cause ID format: {}", e)));
        }
    };
    
    match cause_service.update_cause(&object_id, update_data.into_inner()).await {
        Ok(success) => {
            if success {
                info!("Successfully updated cause");
                Ok(HttpResponse::Ok().body("Cause updated successfully"))
            } else {
                info!("Cause not found for update");
                Ok(HttpResponse::NotFound().body("Cause not found"))
            }
        },
        Err(e) => {
            error!("Failed to update cause: {}", e);
            Err(ErrorInternalServerError(e.to_string()))
        }
    }
}

// Delete a cause
pub async fn delete_cause(
    cause_service: web::Data<CauseService>,
    cause_id: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    info!("Deleting cause with ID: {}", cause_id);
    
    let object_id = match ObjectId::parse_str(cause_id.as_ref()) {
        Ok(id) => id,
        Err(e) => {
            error!("Invalid cause ID format: {}", e);
            return Ok(HttpResponse::BadRequest().body(format!("Invalid cause ID format: {}", e)));
        }
    };
    
    match cause_service.delete_cause(&object_id).await {
        Ok(success) => {
            if success {
                info!("Successfully deleted cause");
                Ok(HttpResponse::Ok().body("Cause deleted successfully"))
            } else {
                info!("Cause not found for deletion");
                Ok(HttpResponse::NotFound().body("Cause not found"))
            }
        },
        Err(e) => {
            error!("Failed to delete cause: {}", e);
            Err(ErrorInternalServerError(e.to_string()))
        }
    }
}

// Get cause by token name
pub async fn get_cause_by_token_name(
    cause_service: web::Data<CauseService>,
    token_name: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    info!("Getting cause by token name: {}", token_name);
    
    match cause_service.get_cause_by_token_name(&token_name).await {
        Ok(cause) => {
            info!("Found cause: {}", cause.name);
            Ok(HttpResponse::Ok().json(cause))
        },
        Err(e) => match e {
            ApiError::NotFound(msg) => {
                info!("{}", msg);
                Ok(HttpResponse::NotFound().body(msg))
            },
            _ => {
                error!("Error retrieving cause: {}", e);
                Err(ErrorInternalServerError(e.to_string()))
            }
        }
    }
}

// Get cause by cause name
pub async fn get_cause_by_name(
    cause_service: web::Data<CauseService>,
    name: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    info!("Getting cause by name: {}", name);
    
    match cause_service.get_cause_by_name(&name).await {
        Ok(cause) => {
            info!("Found cause: {}", cause.name);
            Ok(HttpResponse::Ok().json(cause))
        },
        Err(e) => match e {
            ApiError::NotFound(msg) => {
                info!("{}", msg);
                Ok(HttpResponse::NotFound().body(msg))
            },
            _ => {
                error!("Error retrieving cause: {}", e);
                Err(ErrorInternalServerError(e.to_string()))
            }
        }
    }
}

// Get cause by token symbol
pub async fn get_cause_by_token_symbol(
    cause_service: web::Data<CauseService>,
    token_symbol: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    info!("Getting cause by token symbol: {}", token_symbol);
    
    match cause_service.get_cause_by_token_symbol(&token_symbol).await {
        Ok(cause) => {
            info!("Found cause: {}", cause.name);
            Ok(HttpResponse::Ok().json(cause))
        },
        Err(e) => match e {
            ApiError::NotFound(msg) => {
                info!("{}", msg);
                Ok(HttpResponse::NotFound().body(msg))
            },
            _ => {
                error!("Error retrieving cause: {}", e);
                Err(ErrorInternalServerError(e.to_string()))
            }
        }
    }
}


// Error response struct
#[derive(serde::Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

// Validation request structs
#[derive(serde::Deserialize)]
pub struct ValidateFieldRequest {
    pub value: String,
}

// Validation response struct
#[derive(serde::Serialize)]
pub struct ValidationResponse {
    pub valid: bool,
    pub message: Option<String>,
}

// Draft recovery request
#[derive(serde::Deserialize)]
pub struct FindDraftsRequest {
    pub email: String,
}

// Draft status response
#[derive(serde::Serialize)]
pub struct DraftStatusResponse {
    pub status: String, // "not_found" | "pending" | "incomplete" | "complete"
    pub draft: Option<serde_json::Value>,
    pub onboarding_url: Option<String>,
    pub cause_id: Option<String>,
    pub cause_symbol: Option<String>,
}

// Get onboarding link for a cause
pub async fn get_onboarding_link(
    cause_service: web::Data<CauseService>,
    cause_id: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    info!("Getting onboarding link for cause: {}", cause_id);
    
    match cause_service.create_account_link(&cause_id).await {
        Ok(url) => Ok(HttpResponse::Ok().json(serde_json::json!({
            "onboarding_url": url
        }))),
        Err(e) => {
            error!("Failed to create onboarding link: {}", e);
            match e {
                ApiError::ValidationError(msg) => {
                    Ok(HttpResponse::BadRequest().json(ErrorResponse { 
                        error: "validation_error".to_string(),
                        message: msg,
                    }))
                },
                ApiError::NotFound(msg) => {
                    Ok(HttpResponse::NotFound().json(ErrorResponse { 
                        error: "not_found".to_string(),
                        message: msg,
                    }))
                },
                _ => Err(ErrorInternalServerError(e.to_string()))
            }
        }
    }
}

// Check account status
pub async fn check_account_status(
    cause_service: web::Data<CauseService>,
    cause_id: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    info!("Checking account status for cause: {}", cause_id);
    
    match cause_service.get_account_status(&cause_id).await {
        Ok(status) => Ok(HttpResponse::Ok().json(status)),
        Err(e) => {
            error!("Failed to get account status: {}", e);
            match e {
                ApiError::ValidationError(msg) => {
                    Ok(HttpResponse::BadRequest().json(ErrorResponse { 
                        error: "validation_error".to_string(),
                        message: msg,
                    }))
                },
                ApiError::NotFound(msg) => {
                    Ok(HttpResponse::NotFound().json(ErrorResponse { 
                        error: "not_found".to_string(),
                        message: msg,
                    }))
                },
                _ => Err(ErrorInternalServerError(e.to_string()))
            }
        }
    }
}

// Get draft status
pub async fn get_draft_status(
    cause_service: web::Data<CauseService>,
    draft_id: web::Path<String>,
) -> actix_web::Result<impl Responder> {
    info!("Getting draft status for: {}", draft_id);
    
    match cause_service.get_draft_status(&draft_id).await {
        Ok(response) => Ok(HttpResponse::Ok().json(response)),
        Err(e) => {
            error!("Failed to get draft status: {}", e);
            match e {
                ApiError::NotFound(msg) => {
                    Ok(HttpResponse::Ok().json(DraftStatusResponse {
                        status: "not_found".to_string(),
                        draft: None,
                        onboarding_url: None,
                        cause_id: None,
                        cause_symbol: None,
                    }))
                },
                _ => Err(ErrorInternalServerError(e.to_string()))
            }
        }
    }
}

// Find drafts by email
pub async fn find_drafts_by_email(
    cause_service: web::Data<CauseService>,
    request: web::Json<FindDraftsRequest>,
) -> actix_web::Result<impl Responder> {
    info!("Finding drafts for email: {}", request.email);
    
    match cause_service.find_drafts_by_email(&request.email).await {
        Ok(drafts) => Ok(HttpResponse::Ok().json(drafts)),
        Err(e) => {
            error!("Failed to find drafts: {}", e);
            Err(ErrorInternalServerError(e.to_string()))
        }
    }
}

// Create donation checkout session
pub async fn create_donation_session(
    cause_service: web::Data<CauseService>,
    stripe_client: web::Data<stripe::Client>,
    request: web::Json<CreateDonationSessionRequest>,
) -> actix_web::Result<impl Responder> {
    info!("Creating donation session for cause {} with amount {} cents", 
        request.cause_id, request.amount_cents);
    
    // Get the cause
    let cause_id = match ObjectId::parse_str(&request.cause_id) {
        Ok(id) => id,
        Err(e) => {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                error: "invalid_cause_id".to_string(),
                message: format!("Invalid cause ID: {}", e),
            }));
        }
    };
    
    let cause = match cause_service.get_cause_by_id(&cause_id).await {
        Ok(cause) => cause,
        Err(e) => {
            error!("Failed to get cause: {:?}", e);
            return match e {
                ApiError::NotFound(msg) => {
                    Ok(HttpResponse::NotFound().json(ErrorResponse {
                        error: "cause_not_found".to_string(),
                        message: msg,
                    }))
                },
                _ => Err(ErrorInternalServerError(e.to_string())),
            };
        }
    };
    
    // Get connected account ID
    let connected_account_id = match &cause.stripe_account_id {
        Some(id) => id.clone(),
        None => {
            return Ok(HttpResponse::BadRequest().json(ErrorResponse {
                error: "no_stripe_account".to_string(),
                message: "This cause does not have a connected Stripe account".to_string(),
            }));
        }
    };
    
    // Create checkout session
    match cause_service.create_donation_checkout_session(
        &cause,
        &connected_account_id,
        request.amount_cents,
        &request.user_wallet_address,
    ).await {
        Ok((session_id, checkout_url)) => {
            Ok(HttpResponse::Ok().json(CreateDonationSessionResponse {
                checkout_url,
                session_id,
            }))
        },
        Err(e) => {
            error!("Failed to create checkout session: {:?}", e);
            match e {
                ApiError::ValidationError(msg) => {
                    Ok(HttpResponse::BadRequest().json(ErrorResponse {
                        error: "validation_error".to_string(),
                        message: msg,
                    }))
                },
                _ => Err(ErrorInternalServerError(e.to_string())),
            }
        }
    }
}

// Validate cause name
pub async fn validate_cause_name(
    cause_service: web::Data<CauseService>,
    request: web::Json<ValidateFieldRequest>,
) -> actix_web::Result<impl Responder> {
    let name = request.value.trim();
    
    match cause_service.validate_cause_name(name).await {
        Ok(is_valid) => {
            let response = if is_valid {
                ValidationResponse {
                    valid: true,
                    message: None,
                }
            } else {
                ValidationResponse {
                    valid: false,
                    message: Some("This cause name is already taken".to_string()),
                }
            };
            Ok(HttpResponse::Ok().json(response))
        },
        Err(e) => {
            error!("Failed to validate cause name: {}", e);
            Err(ErrorInternalServerError(e.to_string()))
        }
    }
}

// Validate token symbol
pub async fn validate_token_symbol(
    cause_service: web::Data<CauseService>,
    request: web::Json<ValidateFieldRequest>,
) -> actix_web::Result<impl Responder> {
    let symbol = request.value.trim();
    
    match cause_service.validate_token_symbol(symbol).await {
        Ok(is_valid) => {
            let response = if is_valid {
                ValidationResponse {
                    valid: true,
                    message: None,
                }
            } else {
                ValidationResponse {
                    valid: false,
                    message: Some(if symbol.len() < 2 || symbol.len() > 5 || !symbol.to_uppercase().chars().all(|c| c.is_ascii_uppercase()) {
                        "Token symbol must be 2-5 uppercase letters".to_string()
                    } else {
                        "This token symbol is already taken".to_string()
                    }),
                }
            };
            Ok(HttpResponse::Ok().json(response))
        },
        Err(e) => {
            error!("Failed to validate token symbol: {}", e);
            Err(ErrorInternalServerError(e.to_string()))
        }
    }
}

// Validate token name
pub async fn validate_token_name(
    cause_service: web::Data<CauseService>,
    request: web::Json<ValidateFieldRequest>,
) -> actix_web::Result<impl Responder> {
    let name = request.value.trim();
    
    match cause_service.validate_token_name(name).await {
        Ok(is_valid) => {
            let response = if is_valid {
                ValidationResponse {
                    valid: true,
                    message: None,
                }
            } else {
                ValidationResponse {
                    valid: false,
                    message: Some("This token name is already taken".to_string()),
                }
            };
            Ok(HttpResponse::Ok().json(response))
        },
        Err(e) => {
            error!("Failed to validate token name: {}", e);
            Err(ErrorInternalServerError(e.to_string()))
        }
    }
}