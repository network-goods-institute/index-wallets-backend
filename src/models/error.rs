use serde::Serialize;
use actix_web::{HttpResponse, ResponseError};
use std::fmt;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug)]
pub enum ApiError {
    DuplicateUser(String),
    DuplicateError(String),
    DatabaseError(mongodb::error::Error),
    ValidationError(String),
    NotFound(String),
    StripeError(String),
    InternalError(String),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ApiError::DuplicateUser(msg) => write!(f, "Duplicate user: {}", msg),
            ApiError::DuplicateError(msg) => write!(f, "Duplicate error: {}", msg),
            ApiError::DatabaseError(e) => write!(f, "Database error: {}", e),
            ApiError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            ApiError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ApiError::StripeError(msg) => write!(f, "Stripe error: {}", msg),
            ApiError::InternalError(msg) => write!(f, "Internal error: {}", msg),
        }
    }
}

impl ResponseError for ApiError {
    fn error_response(&self) -> HttpResponse {
        match self {
            ApiError::DuplicateUser(_) => {
                HttpResponse::Conflict().json(ErrorResponse {
                    code: "USER_EXISTS".to_string(),
                    message: self.to_string(),
                    details: None,
                })
            }
            ApiError::DuplicateError(_) => {
                HttpResponse::Conflict().json(ErrorResponse {
                    code: "DUPLICATE_ERROR".to_string(),
                    message: self.to_string(),
                    details: None,
                })
            }
            ApiError::DatabaseError(_) => {
                HttpResponse::InternalServerError().json(ErrorResponse {
                    code: "DATABASE_ERROR".to_string(),
                    message: "Internal server error".to_string(),
                    details: None,
                })
            }
            ApiError::ValidationError(_) => {
                HttpResponse::BadRequest().json(ErrorResponse {
                    code: "VALIDATION_ERROR".to_string(),
                    message: self.to_string(),
                    details: None,
                })
            }
            ApiError::NotFound(_) => {
                HttpResponse::NotFound().json(ErrorResponse {
                    code: "NOT_FOUND".to_string(),
                    message: self.to_string(),
                    details: None,
                })
            }
            ApiError::StripeError(_) => {
                HttpResponse::BadGateway().json(ErrorResponse {
                    code: "STRIPE_ERROR".to_string(),
                    message: self.to_string(),
                    details: None,
                })
            }
            ApiError::InternalError(_) => {
                HttpResponse::InternalServerError().json(ErrorResponse {
                    code: "INTERNAL_ERROR".to_string(),
                    message: self.to_string(),
                    details: None,
                })
            }
        }
    }
} 