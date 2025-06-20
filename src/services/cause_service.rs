use std::sync::Arc;
use std::str::FromStr;
use log::{info, error};
use mongodb::bson::oid::ObjectId;
use futures::stream::TryStreamExt;
use crate::models::cause::{Cause, CauseStatus};
use crate::models::{ApiError, CauseDraft, DraftStatus};
use crate::services::{MongoDBService, TokenService};
use stripe::{Client, PriceId, AccountId, CreateCheckoutSession, CheckoutSessionMode};

// Request and response structs
#[derive(serde::Deserialize)]
pub struct CreateCauseRequest {
    pub name: String,
    pub organization: String,
    pub description: String,
    pub long_description: String,
    pub creator_email: String,
    pub token_name: String,
    pub token_symbol: String,
    pub token_image_url: Option<String>,
    pub cause_image_url: Option<String>,
}

#[derive(serde::Serialize)]
pub struct CreateCauseResponse {
    pub id: String,
}

#[derive(serde::Deserialize)]
pub struct UpdateCauseRequest {
    pub name: Option<String>,
    pub organization: Option<String>,
    pub description: Option<String>,
    pub long_description: Option<String>,
    pub is_active: Option<bool>,
    pub stripe_product_id: Option<String>,
    pub payment_link: Option<String>,
    pub status: Option<CauseStatus>,
    pub token_id: Option<String>,
    pub token_image_url: Option<String>,
    pub cause_image_url: Option<String>,
    pub stripe_account_id: Option<String>,
    pub stripe_account_status: Option<String>,
    pub displayed: Option<bool>,
    pub featured: Option<bool>,
}

pub struct CauseService {
    mongodb_service: Arc<MongoDBService>,
    token_service: Arc<TokenService>,
    stripe_client: Arc<stripe::Client>,
}

impl CauseService {
    pub fn new(
        mongodb_service: Arc<MongoDBService>,
        token_service: Arc<TokenService>,
        stripe_client: Arc<stripe::Client>,
    ) -> Self {
        Self {
            mongodb_service,
            token_service,
            stripe_client,
        }
    }

    // New draft-based cause creation
    pub async fn create_cause(&self, cause_data: CreateCauseRequest) -> Result<serde_json::Value, ApiError> {
        // Validate
        self.validate_cause_data(&cause_data).await
            .map_err(|e| {
                error!("Validation failed: {}", e);
                e
            })?;
        
        let draft = CauseDraft::new(
            cause_data.name.clone(),
            cause_data.organization.clone(),
            cause_data.description.clone(),
            cause_data.long_description.clone(),
            cause_data.creator_email.clone(),
            cause_data.token_name.clone(),
            cause_data.token_symbol.clone(),
            cause_data.token_image_url.clone(),
            cause_data.cause_image_url.clone(),
        );
        
        let draft_id = self.mongodb_service.create_draft(draft.clone())
            .await
            .map_err(|e| {
                // Parse MongoDB duplicate errors to provide specific field information
                let error_msg = e.to_string();
                if error_msg.contains("DUPLICATE_NAME:") {
                    ApiError::DuplicateError("A cause with this name already exists".to_string())
                } else if error_msg.contains("DUPLICATE_TOKEN_NAME:") {
                    ApiError::DuplicateError("A cause with this token name already exists".to_string())
                } else if error_msg.contains("DUPLICATE_TOKEN_SYMBOL:") {
                    ApiError::DuplicateError("A cause with this token symbol already exists".to_string())
                } else {
                    ApiError::DatabaseError(e)
                }
            })?;
        
        info!("Creating Stripe Connected Account for cause: {} (draft_id: {})", cause_data.name, draft_id);
        
        // Create Stripe Connected Account with draft metadata
        let account_params = stripe::CreateAccount {
            type_: Some(stripe::AccountType::Express),
            country: Some("US"),
            email: Some(&cause_data.creator_email),
            capabilities: Some(stripe::CreateAccountCapabilities {
                card_payments: Some(stripe::CreateAccountCapabilitiesCardPayments {
                    requested: Some(true),
                }),
                transfers: Some(stripe::CreateAccountCapabilitiesTransfers {
                    requested: Some(true),
                }),
                ..Default::default()
            }),
            business_type: Some(stripe::AccountBusinessType::Individual),
            metadata: Some([
                ("draft_id".to_string(), draft_id.clone()),
                ("cause_name".to_string(), cause_data.name.clone()),
            ].into()),
            ..Default::default()
        };
        
        info!("Calling Stripe API to create account...");
        let account = match stripe::Account::create(&self.stripe_client, account_params).await {
            Ok(acc) => {
                info!("Successfully created Stripe account with ID: {}", acc.id);
                acc
            },
            Err(e) => {
                error!("Stripe API call failed: {:?}", e);
                error!("Error details - Type: {}, Message: {}", 
                    std::any::type_name_of_val(&e), 
                    e.to_string()
                );
                return Err(ApiError::StripeError(format!("Stripe account creation failed: {}", e)));
            }
        };
        
        // Update draft with Stripe account ID
        let draft_object_id = ObjectId::parse_str(&draft_id)
            .map_err(|_| ApiError::ValidationError("Invalid draft ID".to_string()))?;
            
        self.mongodb_service.update_draft(
            &draft_object_id,
            mongodb::bson::doc! {
                "stripe_account_id": &account.id.to_string(),
                "status": mongodb::bson::to_bson(&DraftStatus::StripePending).unwrap()
            }
        ).await.map_err(ApiError::DatabaseError)?;
        
        // Create onboarding link
        let refresh_url = format!("{}/setup/status?draft={}", 
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()), 
            draft_id
        );
        let return_url = format!("{}/setup/status?draft={}", 
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()), 
            draft_id
        );
        
        let link_params = stripe::CreateAccountLink {
            account: account.id.clone(),
            refresh_url: Some(&refresh_url),
            return_url: Some(&return_url),
            type_: stripe::AccountLinkType::AccountOnboarding,
            collect: None,
            collection_options: None,
            expand: &[],
        };
        
        let link = stripe::AccountLink::create(&self.stripe_client, link_params)
            .await
            .map_err(|e| ApiError::StripeError(e.to_string()))?;
        
        Ok(serde_json::json!({
            "draft_id": draft_id,
            "stripe_account_id": account.id.to_string(),
            "onboarding_url": link.url,
        }))
    }
    
    // Complete cause creation after Stripe onboarding
    pub async fn complete_cause_from_draft(&self, draft_id: &str) -> Result<Cause, ApiError> {
        let object_id = ObjectId::parse_str(draft_id)
            .map_err(|_| ApiError::ValidationError("Invalid draft ID".to_string()))?;
            
        // Get draft
        let draft = self.mongodb_service.get_draft_by_id(&object_id)
            .await
            .map_err(ApiError::DatabaseError)?
            .ok_or_else(|| ApiError::NotFound("Draft not found".to_string()))?;
            
        // Check if draft is already completed
        if draft.status == DraftStatus::Completed {
            // If already completed, get the existing cause
            if let Some(cause_id_str) = draft.cause_id {
                let cause_id = ObjectId::parse_str(&cause_id_str)
                    .map_err(|_| ApiError::ValidationError("Invalid cause ID in draft".to_string()))?;
                let cause = self.mongodb_service.get_cause_by_id(&cause_id)
                    .await
                    .map_err(ApiError::DatabaseError)?
                    .ok_or_else(|| ApiError::NotFound("Cause not found".to_string()))?;
                return Ok(cause);
            }
            return Err(ApiError::ValidationError("Draft marked as completed but no cause ID found".to_string()));
        }
            
        // Verify Stripe account is active
        let account_id = draft.stripe_account_id
            .ok_or_else(|| ApiError::ValidationError("No Stripe account associated with draft".to_string()))?;
            
        let account = stripe::Account::retrieve(
            &self.stripe_client, 
            &stripe::AccountId::from_str(&account_id).map_err(|_| ApiError::ValidationError("Invalid account ID".to_string()))?,
            &[]
        ).await
        .map_err(|e| ApiError::StripeError(e.to_string()))?;
        
        if !account.charges_enabled.unwrap_or(false) || !account.details_submitted.unwrap_or(false) {
            return Err(ApiError::ValidationError("Stripe account onboarding not complete".to_string()));
        }
        
        // Log payouts status for monitoring
        let payouts_enabled = account.payouts_enabled.unwrap_or(false);
        if !payouts_enabled {
            log::warn!("Account {} has charges_enabled but payouts_enabled is false", account_id);
        }
        
        // Create the actual cause using the full flow
        let cause_request = CreateCauseRequest {
            name: draft.name.clone(),
            organization: draft.organization.clone(),
            description: draft.description.clone(),
            long_description: draft.long_description.clone(),
            creator_email: draft.creator_email.clone(),
            token_name: draft.token_name.clone(),
            token_symbol: draft.token_symbol.clone(),
            token_image_url: draft.token_image_url.clone(),
            cause_image_url: draft.cause_image_url.clone(),
        };
        
        let mut cause = self.create_cause_full(cause_request, Some(account_id)).await?;
        
        // Update cause with payouts_enabled status and onboarding completion
        cause.payouts_enabled = payouts_enabled;
        cause.onboarding_completed = true;
        
        // Update the cause in the database with these fields
        if let Some(cause_id) = &cause.id {
            let filter = mongodb::bson::doc! { "_id": cause_id };
            let update = mongodb::bson::doc! { 
                "$set": { 
                    "payouts_enabled": payouts_enabled,
                    "onboarding_completed": true,
                    "updated_at": mongodb::bson::DateTime::from_chrono(chrono::Utc::now())
                } 
            };
            self.mongodb_service.get_causes_collection()
                .update_one(filter, update, None)
                .await
                .map_err(|e| ApiError::DatabaseError(e))?;
        }
        
        // Update draft to mark it as completed and link to the created cause
        self.mongodb_service.update_draft(
            &object_id,
            mongodb::bson::doc! {
                "status": mongodb::bson::to_bson(&DraftStatus::Completed).unwrap(),
                "cause_id": cause.id.as_ref().unwrap().to_string(),
                "completed_at": mongodb::bson::DateTime::from_chrono(chrono::Utc::now())
            }
        ).await.map_err(ApiError::DatabaseError)?;
            
        Ok(cause)
    }
    
    // Original method renamed - used internally after onboarding
    async fn create_cause_full(&self, cause_data: CreateCauseRequest, existing_account_id: Option<String>) -> Result<Cause, ApiError> {
        // Validate and check for duplications
        self.validate_cause_data(&cause_data).await?;
        
        let cause = self.create_pending_cause(&cause_data).await?;
        let cause_id = cause.id.unwrap();
        
        // Use existing account or create new one
        let account_id = if let Some(existing_id) = existing_account_id {
            // Update cause with existing account ID
            self.update_cause_account_id(&cause_id, &existing_id).await?;
            existing_id
        } else {
            // Create Connected Account for the cause creator
            let new_account_id = self.create_connected_account(&cause).await?;
            
            // Update cause with account ID
            self.update_cause_account_id(&cause_id, &new_account_id).await?;
            new_account_id
        };
        
        // Create Stripe product on the platform account (not the connected account)
        let stripe_id = self.create_stripe_product(&cause).await?;
        
        // Create price for the product
        let price_id = self.create_product_price(&stripe_id).await?;
        
        // Skip payment link creation - we use checkout sessions now
        let payment_link = ""; // Empty since we use checkout sessions
        
        // Update cause with Stripe ID and status
        self.update_cause_stripe_id(&cause_id, &stripe_id, &payment_link).await?;
        
 
        let updated_cause = self.get_cause_by_id(&cause_id).await?;
        
        // Mint token
        match self.mint_token_for_cause(&updated_cause).await {
            Ok(token_id) => {
                // Update cause with token ID and set to ACTIVE
                self.finalize_cause(&cause_id, &token_id).await?;
            },
            Err(e) => {
                // Cannot roll back token, but update status to FAILED
                let _ = self.update_cause_status(&cause_id, CauseStatus::Failed, Some(e.to_string())).await;
                return Err(e);
            }
        }
        
        self.get_cause_by_id(&cause_id).await
    }

    // Helper methods
    async fn finalize_cause(&self, cause_id: &ObjectId, token_id: &str) -> Result<(), ApiError> {
        // Update cause with token ID and set to ACTIVE
        self.update_cause_status(cause_id, CauseStatus::Active, None).await?;
        Ok(())
    }
    
    async fn validate_cause_data(&self, cause_data: &CreateCauseRequest) -> Result<(), ApiError> {
        // Basic field validation only - uniqueness is handled by database constraints
        
        // Validate token symbol format (typically 3-5 uppercase letters)
        let symbol = cause_data.token_symbol.trim().to_uppercase();
        if symbol.len() < 2 || symbol.len() > 5 || !symbol.chars().all(|c| c.is_ascii_uppercase()) {
            return Err(ApiError::ValidationError("Token symbol must be 2-5 uppercase letters".to_string()));
        }
        
        // Validate email format (basic check)
        if !cause_data.creator_email.contains('@') {
            return Err(ApiError::ValidationError("Invalid email format".to_string()));
        }
        
        Ok(())
    }
    
    async fn create_pending_cause(&self, cause_data: &CreateCauseRequest) -> Result<Cause, ApiError> {
        // Create a new cause with PENDING status
        let mut cause = Cause::new(
            cause_data.name.clone(),
            cause_data.organization.clone(),
            cause_data.description.clone(),
            cause_data.long_description.clone(),
            cause_data.creator_email.clone(),
            cause_data.token_name.clone(),
            cause_data.token_symbol.clone(),
            cause_data.token_image_url.clone(),
            cause_data.cause_image_url.clone(),
        );
        cause.status = CauseStatus::Pending;

        // Insert into MongoDB
        let id = self.mongodb_service.create_cause(cause.clone()).await
            .map_err(|e| ApiError::DatabaseError(e))?;

        // Convert String to ObjectId and set it
        let object_id = ObjectId::parse_str(&id)
            .map_err(|e| ApiError::DatabaseError(mongodb::error::Error::custom(format!("Failed to parse ObjectId: {}", e))))?;
        cause.id = Some(object_id);

        Ok(cause)
    }
    
    // Temporary method to simulate Stripe product creation
    async fn create_connected_account(&self, cause: &Cause) -> Result<String, ApiError> {
        // Creating Stripe Connected Account
        
        let account_params = stripe::CreateAccount {
            type_: Some(stripe::AccountType::Express),
            country: Some("US"),
            email: Some(&cause.creator_email),
            capabilities: Some(stripe::CreateAccountCapabilities {
                card_payments: Some(stripe::CreateAccountCapabilitiesCardPayments {
                    requested: Some(true),
                }),
                transfers: Some(stripe::CreateAccountCapabilitiesTransfers {
                    requested: Some(true),
                }),
                ..Default::default()
            }),
            business_type: Some(stripe::AccountBusinessType::Individual),
            metadata: Some([
                ("cause_id".to_string(), cause.id.unwrap().to_string()),
                ("cause_name".to_string(), cause.name.clone()),
            ].into()),
            ..Default::default()
        };
        
        match stripe::Account::create(&self.stripe_client, account_params).await {
            Ok(account) => {
                // Successfully created Connected Account
                Ok(account.id.to_string())
            },
            Err(e) => {
                error!("Failed to create Connected Account: {}", e);
                Err(ApiError::StripeError(e.to_string()))
            }
        }
    }

    async fn create_stripe_product(&self, cause: &Cause) -> Result<String, ApiError> {
        // Creating Stripe product

        let product_create_params = stripe::CreateProduct {
            name: &cause.name,
            description: Some(&cause.description),
            metadata: Some([
                ("organization".to_string(), cause.organization.clone()),
                ("token_name".to_string(), cause.token_name.clone()),
                ("token_symbol".to_string(), cause.token_symbol.clone())
            ].into()),
            active: Some(true),
            shippable: Some(false),
            statement_descriptor: None,
            unit_label: None,
            url: None,
            tax_code: None,
            expand: &[],
            images: None,
            package_dimensions: None,
            id: None,
            default_price_data: None,
            features: None,
            type_: None,
        };

        match stripe::Product::create(&self.stripe_client, product_create_params).await {
            Ok(product) => {
                // Successfully created Stripe product
                Ok(product.id.to_string())
            },
            Err(e) => {
                error!("Failed to create Stripe product: {}", e);
                Err(ApiError::StripeError(e.to_string()))
            }
        }
    }

    async fn create_product_price(&self, stripe_id: &str) -> Result<String, ApiError> {
        // Creating Stripe price

        let price_create_params = stripe::CreatePrice {
            currency: stripe::Currency::USD,
            active: Some(true),
            product: Some(stripe::IdOrCreate::Id(stripe_id)),
            unit_amount: None,
            billing_scheme: Some(stripe::PriceBillingScheme::PerUnit),
            currency_options: None,
            custom_unit_amount: Some(stripe::CreatePriceCustomUnitAmount {
                enabled: true,
                maximum: Some(15000), // $150.00
                minimum: Some(100),   // $1.00
                preset: None,
            }),
            expand: &[],
            lookup_key: None,
            metadata: None,
            nickname: None,
            product_data: None,
            recurring: None,
            tax_behavior: None,
            tiers: None,
            tiers_mode: None,
            transfer_lookup_key: None,
            transform_quantity: None,
            unit_amount_decimal: None,
        };

        match stripe::Price::create(&self.stripe_client, price_create_params).await {
            Ok(price) => {
                // Successfully created Stripe price
                Ok(price.id.to_string())
            },
            Err(e) => {
                error!("Failed to create Stripe price: {}", e);
                Err(ApiError::StripeError(e.to_string()))
            }
        }
    }

    async fn update_cause_account_id(&self, cause_id: &ObjectId, account_id: &str) -> Result<(), ApiError> {
        let update = UpdateCauseRequest {
            stripe_account_id: Some(account_id.to_string()),
            stripe_account_status: Some("pending".to_string()),
            name: None,
            organization: None,
            description: None,
            long_description: None,
            is_active: None,
            stripe_product_id: None,
            payment_link: None,
            status: None,
            token_id: None,
            token_image_url: None,
            cause_image_url: None,
            displayed: None,
            featured: None,
        };
        
        self.mongodb_service.update_cause(cause_id, update)
            .await
            .map_err(ApiError::DatabaseError)?;
        
        Ok(())
    }

    async fn update_cause_stripe_id(&self, cause_id: &ObjectId, stripe_id: &str, payment_link: &str) -> Result<(), ApiError> {
        let update = UpdateCauseRequest {
            status: Some(CauseStatus::StripeCreated),
            stripe_product_id: Some(stripe_id.to_string()),
            payment_link: Some(payment_link.to_string()),
            name: None,
            organization: None,
            description: None,
            long_description: None,
            is_active: None,
            token_id: None,
            token_image_url: None,
            cause_image_url: None,
            stripe_account_id: None,
            stripe_account_status: None,
            displayed: None,
            featured: None,
        };
        
        self.mongodb_service.update_cause(cause_id, update)
            .await
            .map_err(ApiError::DatabaseError)?;
        
        Ok(())
    }
    
    // Create an account link for Stripe Connect onboarding
    pub async fn create_account_link(&self, cause_id: &str) -> Result<String, ApiError> {
        let object_id = ObjectId::parse_str(cause_id)
            .map_err(|_| ApiError::ValidationError("Invalid cause ID".to_string()))?;
            
        let cause = self.get_cause_by_id(&object_id).await?;
        
        let account_id = cause.stripe_account_id
            .ok_or_else(|| ApiError::ValidationError("No Stripe account associated with this cause".to_string()))?;
        
        let account_id_obj = stripe::AccountId::from_str(&account_id)
            .map_err(|_| ApiError::ValidationError("Invalid account ID".to_string()))?;
        
        let frontend_url = std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
        let refresh_url = format!("{}/causes/onboarding/refresh?cause_id={}", frontend_url, cause_id);
        let return_url = format!("{}/causes/onboarding/complete?cause_id={}", frontend_url, cause_id);
        
        let account_link_params = stripe::CreateAccountLink {
            account: account_id_obj,
            refresh_url: Some(&refresh_url),
            return_url: Some(&return_url),
            type_: stripe::AccountLinkType::AccountOnboarding,
            collect: None,
            collection_options: None,
            expand: &[],
        };
        
        match stripe::AccountLink::create(&self.stripe_client, account_link_params).await {
            Ok(link) => Ok(link.url),
            Err(e) => Err(ApiError::StripeError(e.to_string())),
        }
    }
    
    // Check the status of a connected account
    pub async fn get_account_status(&self, cause_id: &str) -> Result<serde_json::Value, ApiError> {
        let object_id = ObjectId::parse_str(cause_id)
            .map_err(|_| ApiError::ValidationError("Invalid cause ID".to_string()))?;
            
        let cause = self.get_cause_by_id(&object_id).await?;
        
        let account_id = cause.stripe_account_id
            .ok_or_else(|| ApiError::ValidationError("No Stripe account associated with this cause".to_string()))?;
        
        let account_id_obj = stripe::AccountId::from_str(&account_id)
            .map_err(|_| ApiError::ValidationError("Invalid account ID".to_string()))?;
        match stripe::Account::retrieve(&self.stripe_client, &account_id_obj, &[]).await {
            Ok(account) => {
                let status = serde_json::json!({
                    "charges_enabled": account.charges_enabled.unwrap_or(false),
                    "payouts_enabled": account.payouts_enabled.unwrap_or(false),
                    "details_submitted": account.details_submitted.unwrap_or(false),
                    "account_id": account_id,
                });
                
                // Update cause status in DB
                if account.charges_enabled.unwrap_or(false) && !cause.onboarding_completed {
                    let update = UpdateCauseRequest {
                        stripe_account_status: Some("enabled".to_string()),
                        name: None,
                        organization: None,
                        description: None,
                        long_description: None,
                        is_active: None,
                        stripe_product_id: None,
                        payment_link: None,
                        status: None,
                        token_id: None,
                        token_image_url: None,
                        cause_image_url: None,
                        stripe_account_id: None,
                        displayed: None,
                        featured: None,
                    };
                    let _ = self.mongodb_service.update_cause(&object_id, update).await;
                }
                
                Ok(status)
            },
            Err(e) => Err(ApiError::StripeError(e.to_string())),
        }
    }

    // Get draft status
    pub async fn get_draft_status(&self, draft_id: &str) -> Result<crate::handlers::cause_handlers::DraftStatusResponse, ApiError> {
        let object_id = ObjectId::parse_str(draft_id)
            .map_err(|_| ApiError::ValidationError("Invalid draft ID".to_string()))?;
            
        // Check if draft exists
        if let Some(draft) = self.mongodb_service.get_draft_by_id(&object_id)
            .await
            .map_err(ApiError::DatabaseError)? {
            
            // Check if draft has been completed (cause created)
            if draft.status == DraftStatus::Completed {
                // Get the cause_id from the draft
                if let Some(cause_id_str) = draft.cause_id {
                    return Ok(crate::handlers::cause_handlers::DraftStatusResponse {
                        status: "complete".to_string(),
                        draft: None,
                        onboarding_url: None,
                        cause_id: Some(cause_id_str),
                        cause_symbol: Some(draft.token_symbol.clone()),
                    });
                }
            }
            
            // Check Stripe account status
            if let Some(account_id) = &draft.stripe_account_id {
                let account_id_obj = match stripe::AccountId::from_str(account_id) {
                    Ok(id) => id,
                    Err(_) => return Ok(crate::handlers::cause_handlers::DraftStatusResponse {
                        status: "error".to_string(),
                        draft: Some(serde_json::to_value(&draft).unwrap()),
                        onboarding_url: None,
                        cause_id: None,
                        cause_symbol: Some(draft.token_symbol.clone()),
                    })
                };
                
                match stripe::Account::retrieve(
                    &self.stripe_client,
                    &account_id_obj,
                    &[]
                ).await {
                    Ok(account) => {
                        let status = if account.charges_enabled.unwrap_or(false) && 
                                       account.details_submitted.unwrap_or(false) {
                            "pending" // Ready but not yet processed
                        } else {
                            "incomplete" // Still needs onboarding
                        };
                        
                        // Generate fresh onboarding link if incomplete
                        let onboarding_url = if status == "incomplete" {
                            match self.create_account_link_for_draft(&draft).await {
                                Ok(url) => Some(url),
                                Err(_) => None,
                            }
                        } else {
                            None
                        };
                        
                        Ok(crate::handlers::cause_handlers::DraftStatusResponse {
                            status: status.to_string(),
                            draft: Some(serde_json::to_value(&draft).unwrap()),
                            onboarding_url,
                            cause_id: None,
                            cause_symbol: Some(draft.token_symbol.clone()),
                        })
                    },
                    Err(_) => {
                        // Account retrieval failed
                        Ok(crate::handlers::cause_handlers::DraftStatusResponse {
                            status: "error".to_string(),
                            draft: Some(serde_json::to_value(&draft).unwrap()),
                            onboarding_url: None,
                            cause_id: None,
                            cause_symbol: Some(draft.token_symbol.clone()),
                        })
                    }
                }
            } else {
                // No Stripe account yet
                Ok(crate::handlers::cause_handlers::DraftStatusResponse {
                    status: "draft".to_string(),
                    draft: Some(serde_json::to_value(&draft).unwrap()),
                    onboarding_url: None,
                    cause_id: None,
                    cause_symbol: Some(draft.token_symbol.clone()),
                })
            }
        } else {
            Err(ApiError::NotFound("Draft not found".to_string()))
        }
    }
    
    // Find drafts by email
    pub async fn find_drafts_by_email(&self, email: &str) -> Result<Vec<serde_json::Value>, ApiError> {
        let drafts = self.mongodb_service.find_drafts_by_email(email)
            .await
            .map_err(ApiError::DatabaseError)?;
            
        let mut result = Vec::new();
        for draft in drafts {
            let mut draft_json = serde_json::to_value(&draft).unwrap();
            
            // Add onboarding URL if account exists but incomplete
            if let Some(account_id) = &draft.stripe_account_id {
                if let Ok(account_id_obj) = stripe::AccountId::from_str(account_id) {
                    if let Ok(account) = stripe::Account::retrieve(
                        &self.stripe_client,
                        &account_id_obj,
                        &[]
                    ).await {
                        let needs_onboarding = !account.charges_enabled.unwrap_or(false) || 
                                             !account.details_submitted.unwrap_or(false);
                        
                        if needs_onboarding {
                            if let Ok(url) = self.create_account_link_for_draft(&draft).await {
                                draft_json["onboarding_url"] = serde_json::Value::String(url);
                            }
                        }
                        
                        draft_json["charges_enabled"] = serde_json::Value::Bool(account.charges_enabled.unwrap_or(false));
                        draft_json["details_submitted"] = serde_json::Value::Bool(account.details_submitted.unwrap_or(false));
                    }
                }
            }
            
            result.push(draft_json);
        }
        
        Ok(result)
    }
    
    // Helper to create account link for a draft
    async fn create_account_link_for_draft(&self, draft: &CauseDraft) -> Result<String, ApiError> {
        let account_id = draft.stripe_account_id.as_ref()
            .ok_or_else(|| ApiError::ValidationError("No Stripe account associated with draft".to_string()))?;
            
        let account_id_obj = stripe::AccountId::from_str(account_id)
            .map_err(|_| ApiError::ValidationError("Invalid account ID".to_string()))?;
            
        let refresh_url = format!("{}/setup/status?draft={}", 
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()), 
            draft.id.unwrap().to_string()
        );
        let return_url = format!("{}/setup/status?draft={}", 
            std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string()), 
            draft.id.unwrap().to_string()
        );
            
        let link_params = stripe::CreateAccountLink {
            account: account_id_obj,
            refresh_url: Some(&refresh_url),
            return_url: Some(&return_url),
            type_: stripe::AccountLinkType::AccountOnboarding,
            collect: None,
            collection_options: None,
            expand: &[],
        };
        
        match stripe::AccountLink::create(&self.stripe_client, link_params).await {
            Ok(link) => Ok(link.url),
            Err(e) => Err(ApiError::StripeError(e.to_string())),
        }
    }

    pub async fn get_cause_by_token_name(&self, token_name: &str) -> Result<Cause, ApiError> {
        self.mongodb_service.get_cause_by_token_name(token_name)
            .await
            .map_err(ApiError::DatabaseError)?
            .ok_or_else(|| ApiError::NotFound(format!("Cause not found with token name: {}", token_name)))
    }
    
    pub async fn get_cause_by_name(&self, name: &str) -> Result<Cause, ApiError> {
        self.mongodb_service.get_cause_by_name(name)
            .await
            .map_err(ApiError::DatabaseError)?
            .ok_or_else(|| ApiError::NotFound(format!("Cause not found with name: {}", name)))
    }
    
    pub async fn get_cause_by_token_symbol(&self, token_symbol: &str) -> Result<Cause, ApiError> {
        self.mongodb_service.get_cause_by_token_symbol(token_symbol)
            .await
            .map_err(ApiError::DatabaseError)?
            .ok_or_else(|| ApiError::NotFound(format!("Cause not found with token symbol: {}", token_symbol)))
    }
    
    async fn mint_token_for_cause(&self, cause: &Cause) -> Result<String, ApiError> {
        // Minting token for cause
        
        // Initial supply for the cause token
        let initial_supply = 1_000_000; // 1 million tokens
        
        // Create the token using TokenService - it handles all the configuration internally
        let token = self.token_service.create_token_for_cause(
            &cause.token_name,
            &cause.token_symbol,
            initial_supply,
            cause.token_image_url.clone()
        ).await
        .map_err(|e| ApiError::InternalError(format!("Failed to create token: {}", e)))?;
        
        // Update cause status in MongoDB to ACTIVE since we've completed all steps
        let mut updated_cause = cause.clone();
        updated_cause.status = CauseStatus::Active;
        updated_cause.token_id = Some(token.token_id.clone());
        
        // Update the cause with new status and token ID
        let update = UpdateCauseRequest {
            status: Some(updated_cause.status),
            token_id: updated_cause.token_id,
            name: None,
            organization: None,
            description: None,
            long_description: None,
            is_active: None,
            stripe_product_id: None,
            payment_link: None,
            token_image_url: None,
            cause_image_url: None,
            stripe_account_id: None,
            stripe_account_status: None,
            displayed: None,
            featured: None,
        };
        
        self.mongodb_service.update_cause(&updated_cause.id.unwrap(), update)
            .await
            .map_err(|e| ApiError::DatabaseError(e))?;
        
        Ok(token.token_id)
    }
    
    // Additional methods for CRUD operations
    
    pub async fn get_all_causes(&self) -> Result<Vec<Cause>, ApiError> {
        self.mongodb_service.get_all_causes().await
            .map_err(|e| ApiError::DatabaseError(e))
    }
    
    pub async fn get_featured_causes(&self) -> Result<Vec<Cause>, ApiError> {
        self.mongodb_service.get_featured_causes().await
            .map_err(|e| ApiError::DatabaseError(e))
    }
    
    pub async fn get_all_causes_unfiltered(&self) -> Result<Vec<Cause>, ApiError> {
        self.mongodb_service.get_all_causes_unfiltered().await
            .map_err(|e| ApiError::DatabaseError(e))
    }
    
    pub async fn update_cause_status(&self, cause_id: &ObjectId, status: CauseStatus, error_message: Option<String>) -> Result<(), ApiError> {
        let update = UpdateCauseRequest {
            status: Some(status),
            token_id: None,
            name: None,
            organization: None,
            description: None,
            long_description: None,
            is_active: None,
            stripe_product_id: None,
            payment_link: None,
            token_image_url: None,
            cause_image_url: None,
            stripe_account_id: None,
            stripe_account_status: None,
            displayed: None,
            featured: None,
        };
        
        self.mongodb_service.update_cause(cause_id, update)
            .await
            .map_err(|e| ApiError::DatabaseError(e))
            .map(|_| ())
    }

    pub async fn get_cause_by_id(&self, cause_id: &ObjectId) -> Result<Cause, ApiError> {
        self.mongodb_service.get_cause_by_id(cause_id).await
            .map_err(|e| ApiError::DatabaseError(e))?
            .ok_or_else(|| ApiError::NotFound(format!("Cause not found with ID: {}", cause_id)))
    }

    pub async fn update_cause(&self, cause_id: &ObjectId, update_data: UpdateCauseRequest) -> Result<bool, ApiError> {
        self.mongodb_service.update_cause(cause_id, update_data).await
            .map_err(|e| ApiError::DatabaseError(e))
    }
    
    pub async fn delete_cause(&self, cause_id: &ObjectId) -> Result<bool, ApiError> {
        self.mongodb_service.delete_cause(cause_id).await
            .map_err(|e| ApiError::DatabaseError(e))
    }
    
    // Validation methods for individual fields
    pub async fn validate_cause_name(&self, name: &str) -> Result<bool, ApiError> {
        // Check if name is empty
        if name.trim().is_empty() {
            return Ok(false);
        }
        
        // Check if already taken
        let is_taken = self.mongodb_service.is_cause_name_taken(name)
            .await
            .map_err(ApiError::DatabaseError)?;
        
        Ok(!is_taken)
    }
    
    pub async fn validate_token_symbol(&self, symbol: &str) -> Result<bool, ApiError> {
        // Check format
        let symbol = symbol.trim().to_uppercase();
        if symbol.len() < 2 || symbol.len() > 5 || !symbol.chars().all(|c| c.is_ascii_uppercase()) {
            return Ok(false);
        }
        
        // Check if already taken
        let is_taken = self.mongodb_service.is_token_symbol_taken(&symbol)
            .await
            .map_err(ApiError::DatabaseError)?;
        
        Ok(!is_taken)
    }
    
    pub async fn update_causes_payouts_status(&self, stripe_account_id: &str, payouts_enabled: bool) -> Result<u64, ApiError> {
        let filter = mongodb::bson::doc! {
            "stripe_account_id": stripe_account_id
        };
        let update = mongodb::bson::doc! {
            "$set": {
                "payouts_enabled": payouts_enabled,
                "updated_at": mongodb::bson::DateTime::from_chrono(chrono::Utc::now())
            }
        };
        
        let result = self.mongodb_service.get_causes_collection()
            .update_many(filter, update, None)
            .await
            .map_err(|e| ApiError::DatabaseError(e))?;
            
        Ok(result.modified_count)
    }

    pub async fn validate_token_name(&self, name: &str) -> Result<bool, ApiError> {
        // Check if name is empty
        if name.trim().is_empty() {
            return Ok(false);
        }
        
        // Check if already taken
        let is_taken = self.mongodb_service.is_token_name_taken(name)
            .await
            .map_err(ApiError::DatabaseError)?;
        
        Ok(!is_taken)
    }
    
    // Create a checkout session for donations with destination charges
    pub async fn create_donation_checkout_session(
        &self,
        cause: &Cause,
        connected_account_id: &str,
        amount_cents: i64,
        user_wallet_address: &str,
    ) -> Result<(String, String), ApiError> {
        // Creating donation checkout session
        
        // Validate amount
        if amount_cents < 100 {
            return Err(ApiError::ValidationError("Minimum donation is $1.00".to_string()));
        }
        
        if amount_cents > 999999 {
            return Err(ApiError::ValidationError("Maximum donation is $9,999.99".to_string()));
        }
        
        // Calculate platform fee (5%)
        let platform_fee = (amount_cents as f64 * 0.05).round() as i64;
        
        // Create checkout session params
        let mut params = CreateCheckoutSession::new();
        params.mode = Some(CheckoutSessionMode::Payment);
        
        // Set success and cancel URLs
        let frontend_url = std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());
        let success_url = format!("{}/donation-success?session_id={{CHECKOUT_SESSION_ID}}", frontend_url);
        let cancel_url = format!("{}/causes/{}", frontend_url, cause.id.as_ref().unwrap());
        params.success_url = Some(&success_url);
        params.cancel_url = Some(&cancel_url);
        
        // Set line items with the donation amount
        params.line_items = Some(vec![
            stripe::CreateCheckoutSessionLineItems {
                price_data: Some(stripe::CreateCheckoutSessionLineItemsPriceData {
                    currency: stripe::Currency::USD,
                    product_data: Some(stripe::CreateCheckoutSessionLineItemsPriceDataProductData {
                        name: format!("Donation to {}", cause.name),
                        description: Some(format!("Supporting {}", cause.organization)),
                        images: None,
                        metadata: None,
                        tax_code: None,
                    }),
                    unit_amount: Some(amount_cents),
                    recurring: None,
                    tax_behavior: None,
                    unit_amount_decimal: None,
                    product: None,
                }),
                price: None,
                quantity: Some(1),
                adjustable_quantity: None,
                dynamic_tax_rates: None,
                tax_rates: None,
            }
        ]);
        
        // Set up destination charges
        params.payment_intent_data = Some(stripe::CreateCheckoutSessionPaymentIntentData {
            application_fee_amount: Some(platform_fee),
            transfer_data: Some(stripe::CreateCheckoutSessionPaymentIntentDataTransferData {
                destination: connected_account_id.to_string(),
                amount: None, // Transfer full amount minus application fee
            }),
            capture_method: None,
            metadata: None,
            on_behalf_of: None,
            receipt_email: None,
            setup_future_usage: None,
            shipping: None,
            statement_descriptor: None,
            statement_descriptor_suffix: None,
            transfer_group: None,
            description: None,
        });
        
        // Add metadata for webhook processing
        params.metadata = Some([
            ("cause_id".to_string(), cause.id.as_ref().unwrap().to_string()),
            ("cause_name".to_string(), cause.name.clone()),
            ("token_name".to_string(), cause.token_name.clone()),
            ("token_symbol".to_string(), cause.token_symbol.clone()),
            ("user_wallet_address".to_string(), user_wallet_address.to_string()),
            ("connected_account_id".to_string(), connected_account_id.to_string()),
            ("platform_fee".to_string(), platform_fee.to_string()),
        ].into());
        
        // Set customer email collection
        params.customer_email = None; // We already have wallet address
        
        // Create the session
        match stripe::CheckoutSession::create(&self.stripe_client, params).await {
            Ok(session) => {
                // Successfully created checkout session
                Ok((session.id.to_string(), session.url.unwrap_or_default()))
            },
            Err(e) => {
                error!("Failed to create checkout session: {}", e);
                Err(ApiError::StripeError(e.to_string()))
            }
        }
    }
}
