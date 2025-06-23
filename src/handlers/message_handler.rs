use actix_web::{web, HttpResponse, Responder};
use delta_executor_sdk::base::crypto::Ed25519PubKey;
use delta_executor_sdk::base::vaults::{VaultId, TokenKind, ReadableVault};
use delta_executor_sdk::base::verifiable::debit_allowance::{DebitAllowance, SignedDebitAllowance};
use delta_executor_sdk::base::verifiable::VerifiableType;
use delta_executor_sdk::base::core::Shard;
use serde_json::json;
use crate::models::{Message, User, CreateUserRequest, Preferences, ApiError, Payment, CreatePaymentRequest, PaymentStatus, PaymentIdResponse, SupplementPaymentRequest, SupplementPaymentResponse, TokenPayment, TransactionRecord, TokenValuation, DepositRecord};
use crate::models::payment::{PaymentStatusResponse, ProcessSignedTransactionRequest, TransactionHistoryResponse, TransactionHistoryItem, TransactionDirection, ActivityItem};
use crate::utils::{calculate_vendor_valuations, calculate_payment_bundle, apply_discounts_to_payment, calculate_post_payment_valuations, verify_sufficient_funds_after_discounts};
use crate::utils::payment_code::normalize_payment_code;
use crate::services::{MongoDBService, TokenService, WalletService};
use ed25519_dalek::SigningKey;
use chrono::Utc;
use std::collections::HashSet;
use rand::rngs::OsRng;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use mongodb::bson::Document;
use std::time::{SystemTime, UNIX_EPOCH};
use log;
use std::str::FromStr;
use std::collections::BTreeMap;

pub async fn hello() -> impl Responder {
    HttpResponse::Ok().json(Message {
        content: "Hello, World!".to_string(),
    })
}

pub async fn health_check() -> impl Responder {
    HttpResponse::Ok().json(Message {
        content: "Service is healthy".to_string(),
    })
}

pub async fn echo(msg: web::Json<Message>) -> impl Responder {
    HttpResponse::Ok().json(Message {
        content: msg.content.clone(),
    })
}

pub async fn create_user(
    user_data: web::Json<CreateUserRequest>,
    db: web::Data<MongoDBService>,
) -> Result<HttpResponse, ApiError> {
    let user = User {
        id: None,
        wallet_address: user_data.wallet_address.clone(),
        username: user_data.username.clone(),
        preferences: user_data.preferences.clone().unwrap_or(Preferences(Document::new())),
    };

    let created_user = db.create_user(user).await?;
    Ok(HttpResponse::Created().json(created_user))
}

pub async fn get_user(
    wallet_address: web::Path<String>,
    db: web::Data<MongoDBService>,
) -> Result<HttpResponse, ApiError> {
    match db.get_user_by_wallet(&wallet_address).await? {
        Some(user) => Ok(HttpResponse::Ok().json(user)),
        None => Err(ApiError::NotFound(format!("User with wallet address {} not found", wallet_address)))
    }
}


pub async fn create_payment(
    payment_request: web::Json<CreatePaymentRequest>,
    db: web::Data<MongoDBService>,
) -> Result<HttpResponse, ApiError> {
    log::info!("Received payment request: {:?}", payment_request);

    // Create payment with generated ID and current timestamp
    let payment_id = db.generate_payment_id();
    log::info!("Generated payment ID: {}", payment_id);

    
    let payment = Payment {
        id: None,
        payment_id: payment_id.clone(),
        vendor_address: payment_request.vendor_address.clone(),
        vendor_name: payment_request.vendor_name.clone(),
        price_usd: payment_request.price_usd,
        customer_address: None,
        customer_username: None,
        status: PaymentStatus::Created,
        created_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64,
        vendor_valuations: payment_request.vendor_valuations.clone(),
        discount_consumption: None,
        computed_payment: None,
        initial_payment_bundle: None,
    };

    log::info!("Creating payment in database: {:?}", payment);

    // Store the payment but return ID and other requested fields
    match db.create_payment(payment).await {
        Ok(_) => {
            log::info!("Payment created successfully with ID: {}", payment_id);
            Ok(HttpResponse::Created().json(PaymentIdResponse { 
                payment_id,
                vendor_name: payment_request.vendor_name.clone(),
                price_usd: payment_request.price_usd,
            }))
        },
        Err(e) => {
            log::error!("Failed to create payment: {:?}", e);
            Err(e)
        }
    }
}


pub async fn supplement_transaction(
    payment_id: web::Path<String>,
    supplement_data: web::Json<SupplementPaymentRequest>,
    db: web::Data<MongoDBService>,
    wallet_service: web::Data<WalletService>,
) -> Result<HttpResponse, ApiError> {
    // Normalize the payment code to handle common input errors
    let normalized_payment_id = normalize_payment_code(&payment_id);
    
    log::info!(
        "Supplementing transaction. Payment ID: {} (normalized: {}), Payer Address: {}", 
        payment_id, 
        normalized_payment_id,
        supplement_data.payer_address
    );
    
    let payment = match db.update_payment_with_payer(
        &normalized_payment_id,
        supplement_data.payer_address.clone(),
        supplement_data.payer_username.clone(),
    ).await {
        Ok(payment) => {
            log::info!("Successfully updated payment: {:?}", payment);
            payment
        },
        Err(e) => {
            log::error!("Failed to update payment: {:?}", e);
            return Err(e);
        }
    };

    // Fetch vendor preferences from database
    let vendor_preferences = match db.get_user_preferences(&payment.vendor_address).await {
        Ok(prefs) => prefs,
        Err(e) => {
            log::error!("Failed to get vendor preferences: {:?}", e);
            return Err(e);
        }
    };

    log::info!("Vendor preferences: {:?}", vendor_preferences);
    log::info!("Payer balances: {:?}", supplement_data.payer_balances);
    log::info!("Payment amount: {}", payment.price_usd);
    
    let (vendor_valuations, discount_consumption) = 
        calculate_vendor_valuations(&vendor_preferences, &supplement_data.payer_balances, payment.price_usd);
    
    log::info!("Calculated vendor valuations: {:?}", vendor_valuations);
    log::info!("Calculated discount consumption: {:?}", discount_consumption);

    // Calculate proportional payments before discounts
    let initial_payment_bundle = match calculate_payment_bundle(
        &supplement_data.payer_balances,
        &vendor_valuations,
        payment.price_usd,
    ) {
        Ok(bundle) => bundle,
        Err(e) => {
            log::error!("Failed to calculate payment bundle: {}", e);
            // Simplify error message for frontend
            if e.contains("Insufficient funds") {
                return Err(ApiError::ValidationError("Insufficient funds".to_string()));
            }
            return Err(ApiError::ValidationError("Insufficient funds".to_string()));
        }
    };
    
    // Clone for final payment calculation
    let mut payment_bundle = initial_payment_bundle.clone();
    
    // Apply discounts to the payment
    if let Err(e) = apply_discounts_to_payment(
        &mut payment_bundle,
        &discount_consumption,
        &supplement_data.payer_balances,
    ) {
        log::error!("Failed to apply discounts: {}", e);
        return Err(ApiError::InternalError("Failed to apply discounts".to_string()));
    }

    // Verify sufficient funds after discounts/premiums
    let actual_cost = match verify_sufficient_funds_after_discounts(
        &payment_bundle,
        &supplement_data.payer_balances,
        payment.price_usd,
    ) {
        Ok(cost) => {
            log::info!("Payment feasible. Original price: ${:.2}, Actual cost after adjustments: ${:.2}", 
                payment.price_usd, cost);
            cost
        },
        Err(e) => {
            log::error!("Insufficient funds after vendor adjustments: {}", e);
            return Err(ApiError::ValidationError(e));
        }
    };

    // Clone for response before moving into database update
    let vendor_valuations_for_response = vendor_valuations.clone();
    let discount_consumption_for_response = discount_consumption.clone();

    // Update payment with calculated data (including initial bundle)
    if let Err(e) = db.update_payment_with_calculations(
        &payment_id,
        vendor_valuations,
        discount_consumption,
        payment_bundle.clone(),
        initial_payment_bundle.clone(),
    ).await {
        log::error!("Failed to update payment with calculations: {:?}", e);
        return Err(e);
    }

    // Generate unsigned transaction
    let unsigned_transaction = match generate_unsigned_transaction(
        wallet_service.get_ref(),
        &supplement_data.payer_address,
        &payment.vendor_address,
        &payment_bundle
    ).await {
        Ok(tx) => tx,
        Err(e) => {
            log::error!("Failed to generate unsigned transaction: {}", e);
            return Err(ApiError::InternalError(format!("Failed to generate transaction: {}", e)));
        }
    };

    // Step 7: Return response with payment bundle for signing
    let response = SupplementPaymentResponse {
        payment_id: payment.payment_id,
        vendor_address: payment.vendor_address,
        vendor_name: payment.vendor_name,
        customer_address: payment.customer_address,
        status: PaymentStatus::Calculated, // Update status
        price_usd: payment.price_usd,
        created_at: payment.created_at,
        payment_bundle, // Easy display for UI 
        unsigned_transaction,
        vendor_valuations: Some(vendor_valuations_for_response),
        discount_consumption: Some(discount_consumption_for_response),
    };

    log::info!("Returning calculated payment: {:?}", response);
    Ok(HttpResponse::Ok().json(response))
}

pub async fn process_signed_transaction(
    payment_id: web::Path<String>, 
    supplement_data: web::Json<ProcessSignedTransactionRequest>, 
    db: web::Data<MongoDBService>,
    wallet_service: web::Data<WalletService>
) -> Result<HttpResponse, ApiError> { 
    log::info!("Processing signed transaction for payment ID: {}", payment_id);
    log::info!("Full request body: {:?}", supplement_data);
    
    // Verify payment ID matches
    if payment_id.to_string() != supplement_data.payment_id {
        log::error!("Payment ID mismatch: {} vs {}", payment_id, supplement_data.payment_id);
        return Err(ApiError::ValidationError("Payment ID mismatch".to_string()));
    }
    
    // Submit the signed transaction to the executor
    let signed_debit_allowances = match serde_json::from_str::<Vec<SignedDebitAllowance>>(&supplement_data.signed_transaction) {
        Ok(allowances) => allowances,
        Err(e) => {
            log::error!("Failed to parse signed transaction: {}", e);
            return Err(ApiError::ValidationError(format!("Invalid signed transaction format: {}", e)));
        }
    };
    
    log::info!("Submitting {} signed debit allowances", signed_debit_allowances.len());
    
    // Convert to VerifiableType and submit
    let verifiables: Vec<VerifiableType> = signed_debit_allowances
        .into_iter()
        .map(|allowance| VerifiableType::DebitAllowance(allowance))
        .collect();
    
    match wallet_service.submit_verifiables(verifiables).await {
        Ok(_) => {
            log::info!("Successfully submitted transaction for payment ID: {}", payment_id);
            
            // Update payment status to completed
            match db.update_payment_status(&payment_id, PaymentStatus::Completed).await {
                Ok(_) => {
                    log::info!("Updated payment status to Completed for payment ID: {}", payment_id);
                    
                    // Perform post-transaction processing
                    // 1. Update vendor preferences (subtract discount amounts consumed)
                    log::info!("Updating vendor preferences after payment completion");
                    
                    // Get the payment to retrieve vendor address and discount consumption data
                    match db.get_payment_by_id(&payment_id).await {
                        Ok(payment) => {
                            if let Some(discount_consumption) = &payment.discount_consumption {
                                // Update VENDOR's preferences with consumed discounts (NO effective valuations)
                                if let Err(e) = db.update_user_preferences_after_payment(
                                    &payment.vendor_address,  // Use vendor address, not payer!
                                    discount_consumption,
                                    None,  // Don't update effective valuations in preferences
                                ).await {
                                    log::error!("Failed to update vendor preferences after payment: {}", e);
                                    // Don't fail the transaction, just log the error
                                }
                            }
                        },
                        Err(e) => {
                            log::error!("Failed to retrieve payment for discount consumption: {}", e);
                        }
                    }
                    
                    // 2. Flatten each token payment into multiple transactions to save
                    log::info!("Processing payment bundle with {} token payments", supplement_data.payment_bundle.len());
                    for token_payment in &supplement_data.payment_bundle {
                        log::info!("Processed token payment: {} {}", token_payment.amount_to_pay, token_payment.symbol);
                        // This would be implemented in a future phase
                    }
                    
                    // 3. Update token market values
                    log::info!("Updating market values for tokens used in transaction");
                    
                    // Task 1: Create flattened transaction records with effective valuations
                    match db.get_payment_by_id(&payment_id).await {
                        Ok(payment) => {
                            if let Some(initial_bundle) = &payment.initial_payment_bundle {
                                let mut effective_valuations = Vec::new();
                                
                                for final_payment in &supplement_data.payment_bundle {
                                    if let Some(initial_payment) = initial_bundle.iter()
                                        .find(|p| p.token_key == final_payment.token_key) {
                                        
                                        if initial_payment.amount_to_pay > 0.0 {
                                            let effective_val = final_payment.amount_to_pay / initial_payment.amount_to_pay;
                                            effective_valuations.push((final_payment.symbol.clone(), effective_val));
                                        }
                                    }
                                }
                                
                                if let Err(e) = create_transaction_records_with_effective_valuations(
                                    &db,
                                    &supplement_data.payment_bundle,
                                    &effective_valuations,
                                    &payment_id
                                ).await {
                                    log::error!("Failed to create transaction records: {}", e);
                                }
                            } else {
                                // Missing data, use vendor valuations if available
                                if let Some(vendor_valuations) = &payment.vendor_valuations {
                                    if let Err(e) = create_transaction_records_with_vendor_valuations(
                                        &db,
                                        &supplement_data.payment_bundle,
                                        vendor_valuations,
                                        &payment_id
                                    ).await {
                                        log::error!("Failed to create transaction records: {}", e);
                                    }
                                } else {
                                    // No valuations at all, use simple records
                                    if let Err(e) = create_transaction_records_simple(
                                        &db,
                                        &supplement_data.payment_bundle,
                                        &payment_id
                                    ).await {
                                        log::error!("Failed to create transaction records: {}", e);
                                    }
                                }
                            }
                        },
                        Err(e) => {
                            log::error!("Failed to get payment for transaction records: {}", e);
                            // Fallback to simple records
                            if let Err(e) = create_transaction_records_simple(
                                &db,
                                &supplement_data.payment_bundle,
                                &payment_id
                            ).await {
                                log::error!("Failed to create transaction records: {}", e);
                            }
                        }
                    }
                    
                    // Task 2: Update market prices
                    if let Err(e) = update_market_prices(&db, &supplement_data.payment_bundle).await {
                        log::error!("Failed to update market prices: {}", e);
                        // Don't fail the whole transaction for this
                    }
                    
                    // Task 3: Update vendor preferences (we'll skip this for now due to storage complexity)
                    log::info!("Vendor preference updates skipped due to data format complexities");
                    
                    // Return success response with full transaction details using request data
                    let response = PaymentStatusResponse {
                        payment_id: payment_id.to_string(),
                        vendor_address: supplement_data.vendor_address.clone(),
                        vendor_name: supplement_data.vendor_name.clone(),
                        customer_address: Some(supplement_data.payer_address.clone()),
                        status: PaymentStatus::Completed, // Updated status
                        price_usd: supplement_data.price_usd,
                        created_at: chrono::Utc::now().timestamp(),
                        payment_bundle: Some(supplement_data.payment_bundle.clone()),
                        computed_payment: Some(supplement_data.payment_bundle.clone()),
                        vendor_valuations: None, // Could add if needed
                        discount_consumption: None, // Could add if needed
                    };
                    
                    Ok(HttpResponse::Ok().json(response))
                },
                Err(e) => {
                    log::error!("Failed to update payment status: {}", e);
                    // Transaction was submitted successfully, but payment status update failed
                    // Return partial success with transaction details from request data
                    let response = PaymentStatusResponse {
                        payment_id: payment_id.to_string(),
                        vendor_address: supplement_data.vendor_address.clone(),
                        vendor_name: supplement_data.vendor_name.clone(),
                        customer_address: Some(supplement_data.payer_address.clone()),
                        status: PaymentStatus::Calculated, // Status wasn't updated due to error
                        price_usd: supplement_data.price_usd,
                        created_at: chrono::Utc::now().timestamp(),
                        payment_bundle: Some(supplement_data.payment_bundle.clone()),
                        computed_payment: Some(supplement_data.payment_bundle.clone()),
                        vendor_valuations: None,
                        discount_consumption: None,
                    };
                    
                    Ok(HttpResponse::Ok().json(json!({
                        "status": "partial_success",
                        "message": "Transaction submitted successfully but payment status update failed",
                        "error": format!("Failed to update payment status: {}", e),
                        "transaction": response
                    })))
                }
            }
        },
        Err(e) => {
            log::error!("Failed to submit transaction: {}", e);
            Err(ApiError::InternalError(format!("Failed to submit transaction: {}", e)))
        }
    }
}


pub async fn get_payment_status(
    payment_id: web::Path<String>,
    db: web::Data<MongoDBService>,
) -> Result<HttpResponse, ApiError> {
    // Normalize the payment code to handle common input errors
    let normalized_payment_id = normalize_payment_code(&payment_id);
    
    log::info!("=== PAYMENT STATUS REQUEST ===");
    log::info!("Requested payment ID: {} (normalized: {})", payment_id, normalized_payment_id);

    let payment = match db.get_payment(&normalized_payment_id).await {
        Ok(Some(payment)) => {
            log::info!("✅ Payment found in database");
            log::info!("Payment ID: {}", payment.payment_id);
            log::info!("Vendor Address: {}", payment.vendor_address);
            log::info!("Vendor Name: {}", payment.vendor_name);
            log::info!("Customer Address: {:?}", payment.customer_address);
            log::info!("Status: {:?}", payment.status);
            log::info!("Price USD: {}", payment.price_usd);
            log::info!("Created At: {}", payment.created_at);
            log::info!("Has vendor_valuations: {}", payment.vendor_valuations.is_some());
            log::info!("Has discount_consumption: {}", payment.discount_consumption.is_some());
            log::info!("Has computed_payment: {}", payment.computed_payment.is_some());
            
            if let Some(ref computed) = payment.computed_payment {
                log::info!("Computed payment has {} tokens", computed.len());
                for (i, token) in computed.iter().enumerate() {
                    log::info!("  Token {}: {} {} (amount: {})", i, token.symbol, token.token_key, token.amount_to_pay);
                }
            }
            
            if let Some(ref vendor_vals) = payment.vendor_valuations {
                log::info!("Vendor valuations: {:?}", vendor_vals);
            }
            
            payment
        },
        Ok(None) => {
            log::error!("❌ Payment not found for ID: {}", payment_id);
            return Err(ApiError::NotFound(format!("Payment with ID {} not found", payment_id)));
        },
        Err(e) => {
            log::error!("❌ Database error retrieving payment {}: {}", payment_id, e);
            return Err(e);
        }
    };


    // Log vendor valuations details
    if let Some(ref valuations) = payment.vendor_valuations {
        log::info!("✅ Payment has {} vendor valuations", valuations.len());
        for (i, val) in valuations.iter().enumerate() {
            log::info!("  Valuation {}: {} = {}", i, val.symbol, val.valuation);
        }
    } else {
        log::info!("⚠️ Payment has no vendor valuations");
    }

    let response = PaymentStatusResponse {
        payment_id: payment.payment_id.clone(),
        vendor_address: payment.vendor_address.clone(),
        vendor_name: payment.vendor_name.clone(),
        customer_address: payment.customer_address.clone(),
        status: payment.status.clone(),
        price_usd: payment.price_usd,
        created_at: payment.created_at,
        payment_bundle: payment.computed_payment.clone(),
        computed_payment: payment.computed_payment.clone(),
        vendor_valuations: payment.vendor_valuations.clone(),
        discount_consumption: payment.discount_consumption.clone(),
    };

    log::info!("=== PAYMENT STATUS RESPONSE ===");
    log::info!("Response payment_id: {}", response.payment_id);
    log::info!("Response status: {:?}", response.status);
    log::info!("Response has payment_bundle: {}", response.payment_bundle.is_some());
    log::info!("Response has computed_payment: {}", response.computed_payment.is_some());
    log::info!("Response has vendor_valuations: {}", response.vendor_valuations.is_some());
    log::info!("Response has discount_consumption: {}", response.discount_consumption.is_some());
    
    if let Some(ref bundle) = response.payment_bundle {
        log::info!("Payment bundle contains {} tokens", bundle.len());
    }
    
    log::info!("Sending HTTP 200 OK response");
    Ok(HttpResponse::Ok().json(response))
}

// Helper function to generate unsigned transaction from payment bundle
async fn generate_unsigned_transaction(
    wallet_service: &WalletService,
    payer_address: &str,
    vendor_address: &str,
    payment_bundle: &[TokenPayment],
) -> Result<String, String> {
    log::info!("Generating unsigned transaction for payer: {}, vendor: {}", payer_address, vendor_address);
    
    // Parse payer and vendor addresses
    let payer_pubkey = match Ed25519PubKey::from_str(payer_address) {
        Ok(pk) => pk,
        Err(e) => return Err(format!("Invalid payer address format: {}", e)),
    };
    
    let vendor_pubkey = match Ed25519PubKey::from_str(vendor_address) {
        Ok(pk) => pk,
        Err(e) => return Err(format!("Invalid vendor address format: {}", e)),
    };
    
    // Create a list to hold all debit allowances
    let mut debit_allowances = Vec::with_capacity(payment_bundle.len());
    
    // Get the payer's vault to check current nonce
    let payer_vault = match wallet_service.get_vault(&payer_pubkey).await {
        Ok(Some(vault)) => vault,
        Ok(None) => return Err(format!("Vault not found for payer address: {}", payer_pubkey)),
        Err(e) => return Err(format!("Failed to get payer vault: {}", e)),
    };
    
    // Get current nonce from the vault
    let current_nonce = payer_vault.nonce();
    
    // Default shard ID (using 1 as in the example)
    let shard = Shard::from(1u64);
    
    // Create vault IDs for payer and vendor
    let from_vault_id = VaultId::new(payer_pubkey, shard);
    let to_vault_id = VaultId::new(vendor_pubkey, shard);
    
    // Create allowances map for all tokens
    let mut allowances = BTreeMap::new();
    
    // Process each token payment
    for (index, token_payment) in payment_bundle.iter().enumerate() {
        log::info!("Processing token payment: {:?}", token_payment);
        
        // Parse token key (format: "pubkey,shard")
        let token_parts: Vec<&str> = token_payment.token_key.split(',').collect();
        if token_parts.len() != 2 {
            return Err(format!("Invalid token key format: {}", token_payment.token_key));
        }
        
        // Parse token pubkey
        let token_pubkey = match Ed25519PubKey::from_str(token_parts[0]) {
            Ok(pk) => pk,
            Err(e) => return Err(format!("Invalid token pubkey: {}", e)),
        };
        
        // Parse shard ID
        let token_shard_id = match token_parts[1].parse::<u64>() {
            Ok(id) => Shard::from(id),
            Err(e) => return Err(format!("Invalid shard ID: {}", e)),
        };
        
        // Create token vault ID
        let token_vault_id = VaultId::new(token_pubkey, token_shard_id);
        
        // Convert floating point amount to integer (multiply by 100 and round)
        // For example: 3.89 -> 389
        let amount = (token_payment.amount_to_pay * 100.0).round() as u64;
        
        // Add this token to the allowances map
        allowances.insert(TokenKind::NonNative(token_vault_id), amount);
        
        log::info!("Added token to allowances: token_id={}, amount={}", token_vault_id, amount);
    }
    
    // Create a single debit allowance with all token allowances
    let debit_allowance = DebitAllowance {
        debited: from_vault_id,
        credited: to_vault_id,
        new_nonce: current_nonce + 1, // Incrementing the current nonce
        allowances,
    };
    
    log::info!("Created debit allowance: debited={}, credited={}", 
              debit_allowance.debited, debit_allowance.credited);
    
    debit_allowances.push(debit_allowance);
    
    // Serialize the list of debit allowances to JSON
    match serde_json::to_string(&debit_allowances) {
        Ok(json) => {
            log::info!("Generated unsigned transaction JSON: {}", json);
            Ok(json)
        },
        Err(e) => Err(format!("Failed to serialize debit allowances: {}", e)),
    }
}

// pub async fn complete_transaction(
//     payment_id: web::Path<String>,
//     db: web::Data<MongoDBService>,
// ) -> Result<HttpResponse, ApiError> {
//     log::info!("Completing transaction for payment ID: {}", payment_id);

//     let payment = match db.complete_payment(&payment_id).await {
//         Ok(payment) => {
//             log::info!("Successfully completed payment: {:?}", payment);
//             payment
//         },
//         Err(e) => {
//             log::error!("Failed to complete payment: {:?}", e);
//             return Err(e);
//         }
//     };

//     let response = SupplementPaymentResponse {
//         payment_id: payment.payment_id,
//         vendor_address: payment.vendor_address,
//         vendor_name: payment.vendor_name,
//         status: payment.status,
//         price_usd: payment.price_usd,
//         created_at: payment.created_at,
//     };

//     log::info!("Returning completed payment status: {:?}", response);
//     Ok(HttpResponse::Ok().json(response))
// }

// Helper functions for transaction records and market price updates

async fn create_transaction_records_simple(
    db: &MongoDBService,
    payment_bundle: &[TokenPayment],
    payment_id: &str
) -> Result<(), ApiError> {
    log::info!("Creating transaction records for payment {}", payment_id);
    log::info!("Payment bundle has {} tokens", payment_bundle.len());
    
    // For each token in payment_bundle, create a transaction record with default valuation
    for token_payment in payment_bundle {
        let record = TransactionRecord {
            id: None,
            token_key: token_payment.token_key.clone(),
            symbol: token_payment.symbol.clone(),
            amount_paid: token_payment.amount_to_pay,
            effective_valuation: 1.0, // Default valuation - will be improved in future iteration
            timestamp: Utc::now(),
            payment_id: payment_id.to_string(),
        };
        
        match db.create_transaction_record(record).await {
            Ok(_) => log::info!("Created transaction record for token {}", token_payment.symbol),
            Err(e) => log::error!("Failed to create transaction record for token {}: {}", token_payment.symbol, e),
        }
    }
    
    Ok(())
}

async fn update_market_prices(
    db: &MongoDBService,
    payment_bundle: &[TokenPayment]
) -> Result<(), ApiError> {
    // Get unique token keys
    let unique_tokens: HashSet<String> = payment_bundle
        .iter()
        .map(|token| token.token_key.clone())
        .collect();
    
    log::info!("Updating market prices for {} unique tokens", unique_tokens.len());
    
    // For each unique token, calculate new market price
    for token_key in unique_tokens {
        match calculate_new_market_price(db, &token_key).await {
            Ok(new_price) => {
                log::info!("Calculated new market price for {}: {}", token_key, new_price);
                if let Err(e) = db.update_token_market_price(&token_key, new_price).await {
                    log::error!("Failed to update market price for {}: {}", token_key, e);
                }
            },
            Err(e) => {
                log::error!("Failed to calculate market price for {}: {}", token_key, e);
            }
        }
    }
    
    Ok(())
}

async fn calculate_new_market_price(
    db: &MongoDBService,
    token_key: &str
) -> Result<f64, ApiError> {
    // Get last 20 transaction records for this token
    let records = db.get_recent_transactions_for_token(token_key, 20).await?;
    
    if records.is_empty() {
        return Err(ApiError::InternalError("No transaction records found for token".to_string()));
    }
    
    log::info!("Found {} transaction records for token {}", records.len(), token_key);
    
    // Calculate weighted average using linear decay
    let mut weighted_sum = 0.0;
    let mut weight_sum = 0.0;
    
    for (i, record) in records.iter().enumerate() {
        // Linear decay: weight[i] = (20 - i) / 20
        let weight = (20.0 - i as f64) / 20.0;
        
        weighted_sum += record.effective_valuation * record.amount_paid * weight;
        weight_sum += record.amount_paid * weight;
    }
    
    if weight_sum == 0.0 {
        return Err(ApiError::InternalError("Zero weight sum in market price calculation".to_string()));
    }
    
    let new_market_price = weighted_sum / weight_sum;
    log::info!("Calculated weighted market price: {} (from {} records)", new_market_price, records.len());
    
    Ok(new_market_price)
}

async fn create_transaction_records_with_effective_valuations(
    db: &MongoDBService,
    payment_bundle: &[TokenPayment],
    effective_valuations: &[(String, f64)],
    payment_id: &str
) -> Result<(), ApiError> {
    log::info!("Creating transaction records with effective valuations for payment {}", payment_id);
    log::info!("Payment bundle has {} tokens", payment_bundle.len());
    
    // For each token in payment_bundle, create a transaction record with effective valuation
    for token_payment in payment_bundle {
        // Find the corresponding effective valuation for this token
        let effective_valuation = effective_valuations.iter()
            .find(|(symbol, _)| symbol == &token_payment.symbol)
            .map(|(_, val)| *val)
            .unwrap_or(1.0); // Fallback to 1.0 if no effective valuation found
        
        let record = TransactionRecord {
            id: None,
            token_key: token_payment.token_key.clone(),
            symbol: token_payment.symbol.clone(),
            amount_paid: token_payment.amount_to_pay,
            effective_valuation, // Use the calculated effective valuation
            timestamp: Utc::now(),
            payment_id: payment_id.to_string(),
        };
        
        match db.create_transaction_record(record).await {
            Ok(_) => log::info!("Created transaction record for token {} with effective valuation {}", 
                token_payment.symbol, effective_valuation),
            Err(e) => log::error!("Failed to create transaction record for token {}: {}", 
                token_payment.symbol, e),
        }
    }
    
    Ok(())
}

async fn create_transaction_records_with_vendor_valuations(
    db: &MongoDBService,
    payment_bundle: &[TokenPayment],
    vendor_valuations: &[TokenValuation],
    payment_id: &str
) -> Result<(), ApiError> {
    log::info!("Creating transaction records with vendor valuations for payment {}", payment_id);
    log::info!("Payment bundle has {} tokens", payment_bundle.len());
    
    // For each token in payment_bundle, create a transaction record with vendor valuation
    for token_payment in payment_bundle {
        // Find the corresponding vendor valuation for this token
        let effective_valuation = vendor_valuations.iter()
            .find(|v| v.symbol == token_payment.symbol)
            .map(|v| v.valuation)
            .unwrap_or(1.0); // Fallback to 1.0 if no vendor valuation found
        
        let record = TransactionRecord {
            id: None,
            token_key: token_payment.token_key.clone(),
            symbol: token_payment.symbol.clone(),
            amount_paid: token_payment.amount_to_pay,
            effective_valuation, // Use vendor's valuation (without discount effects)
            timestamp: Utc::now(),
            payment_id: payment_id.to_string(),
        };
        
        match db.create_transaction_record(record).await {
            Ok(_) => log::info!("Created transaction record for token {} with vendor valuation {}", 
                token_payment.symbol, effective_valuation),
            Err(e) => log::error!("Failed to create transaction record for token {}: {}", 
                token_payment.symbol, e),
        }
    }
    
    Ok(())
}


pub async fn get_user_transaction_history(
    user_address: web::Path<String>,
    db: web::Data<MongoDBService>,
) -> Result<HttpResponse, ApiError> {
    log::info!("Getting transaction history for user: {}", user_address);

    // Get both payments and deposits
    let payments = db.get_user_transaction_history(&user_address).await?;
    let deposits = db.get_user_deposits(&user_address).await?;
    
    // Convert payments to ActivityItems
    let mut activities: Vec<(i64, ActivityItem)> = payments
        .into_iter()
        .map(|payment| {
            // Determine direction, counterparty address and username
            let (direction, counterparty_address, counterparty_username) = if payment.vendor_address == *user_address {
                // User is the vendor (received payment)
                (
                    TransactionDirection::Received, 
                    payment.customer_address.clone().unwrap_or("Unknown".to_string()),
                    payment.customer_username.clone()
                )
            } else {
                // User is the customer (sent payment)
                // For sent transactions, the vendor_name is effectively the username
                (
                    TransactionDirection::Sent, 
                    payment.vendor_address.clone(),
                    Some(payment.vendor_name.clone())
                )
            };

            let transaction_item = TransactionHistoryItem {
                payment_id: payment.payment_id,
                direction,
                counterparty_address,
                counterparty_username,
                vendor_name: payment.vendor_name,
                status: payment.status,
                price_usd: payment.price_usd,
                created_at: payment.created_at,
                computed_payment: payment.computed_payment,
            };
            
            (payment.created_at, ActivityItem::Transaction(transaction_item))
        })
        .collect();
    
    // Convert deposits to ActivityItems and add to the list
    for deposit in deposits {
        activities.push((deposit.created_at, ActivityItem::Deposit(deposit)));
    }
    
    // Sort by timestamp descending (newest first)
    activities.sort_by(|a, b| b.0.cmp(&a.0));
    
    // Extract just the ActivityItems
    let sorted_activities: Vec<ActivityItem> = activities.into_iter().map(|(_, item)| item).collect();

    let response = TransactionHistoryResponse { 
        activities: sorted_activities
    };
    
    log::info!("Returning {} activities for user {}", 
              response.activities.len(), user_address);
    Ok(HttpResponse::Ok().json(response))
}

pub async fn delete_payment(
    db: web::Data<MongoDBService>,
    payment_id: web::Path<String>,
    req: web::Json<DeletePaymentRequest>,
) -> Result<HttpResponse, ApiError> {
    log::info!("Deleting payment {} by vendor {}", payment_id.as_str(), req.vendor_address);
    
    db.delete_payment(payment_id.as_str(), &req.vendor_address).await?;
    
    Ok(HttpResponse::Ok().json(json!({
        "message": "Payment cancelled successfully"
    })))
}

#[derive(serde::Deserialize)]
pub struct DeletePaymentRequest {
    pub vendor_address: String,
}

