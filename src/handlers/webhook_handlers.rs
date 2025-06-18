use actix_web::{web, HttpRequest, HttpResponse};
use log::{info, error};
use stripe::{Webhook, EventObject, EventType};

use crate::services::{WebhookService, CauseService, MongoDBService};
use crate::models::{WebhookError, DepositRecord};

pub async fn handle_stripe_webhook(
    req: HttpRequest,
    payload: web::Bytes,
    webhook_service: web::Data<WebhookService>,
    cause_service: web::Data<CauseService>,
    mongodb_service: web::Data<MongoDBService>,
) -> HttpResponse {
    info!("=== STRIPE WEBHOOK RECEIVED ===");
    match process_stripe_webhook(&req, &payload, webhook_service, cause_service, mongodb_service).await {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(e) => {
            error!("Webhook error: {:?}", e);
            HttpResponse::InternalServerError().body(format!("Webhook error: {:?}", e))
        }
    }
}

async fn process_stripe_webhook(
    req: &HttpRequest,
    payload: &web::Bytes,
    webhook_service: web::Data<WebhookService>,
    cause_service: web::Data<CauseService>,
    mongodb_service: web::Data<MongoDBService>,
) -> Result<(), WebhookError> {
    // 1. Get payload as str
    let payload_str = std::str::from_utf8(payload.as_ref())
        .map_err(|e| WebhookError::InvalidPayload(e.to_string()))?;

    // 2. Get the signature header
    let stripe_signature = get_header_value(&req, "Stripe-Signature")
        .ok_or_else(|| WebhookError::MissingSignature)?;

    // 3. Verify & parse the event
    let event = Webhook::construct_event(
        payload_str,
        stripe_signature,
        webhook_service.get_stripe_secret(),
    )?;

    // 4. Handle the event based on its type
    match event.type_ {
        EventType::CheckoutSessionCompleted => {
            if let EventObject::CheckoutSession(sess) = event.data.object {
                let session_id = &sess.id;

                // Get user's wallet address from metadata or client reference ID
                // Payment links with custom fields will populate the metadata
                let client_ref = sess
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("user_wallet_address"))
                    .map(String::as_str)
                    .or_else(|| sess.client_reference_id.as_deref())
                    .unwrap_or("none");

                // Get total amount
                let total = sess
                    .amount_total
                    .unwrap_or(0);

                // Get token symbol from metadata
                let token_symbol = sess
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("token_symbol"))
                    .map(String::as_str)
                    .unwrap_or("unknown");
                
                // Also get token name for logging
                let token_name = sess
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("token_name"))
                    .map(String::as_str)
                    .unwrap_or("unknown");

                info!("received checkout.session.completed → {}", session_id);
                info!("from id: {}", client_ref);
                info!("for amount: {} cents", total);
                info!("for token: {} ({})", token_name, token_symbol);
                
                // Check if this is a USD topup
                // USD payments without a connected account are topups
                let is_usd = token_symbol == "USD";
                let has_connected_account = sess
                    .metadata
                    .as_ref()
                    .and_then(|m| m.get("connected_account_id"))
                    .is_some();
                let is_topup = is_usd && !has_connected_account;
                
                // Save deposit record
                let amount_usd = total as f64 / 100.0;
                let tokens_received = if is_topup {
                    info!("Payment type: USD topup - full amount credited to user");
                    total as f64 // USD 1:1
                } else {
                    // Calculate fee split for logging (donations only)
                    let platform_fee = (total as f64 * 0.05).round() as i64;
                    let amount_to_cause = total - platform_fee;
                    info!("Payment type: Donation");
                    info!("platform fee: {} cents (5%)", platform_fee);
                    info!("amount to cause: {} cents (95%)", amount_to_cause);

                    // With destination charges, Stripe automatically handles the transfer
                    // No manual transfer needed - the connected account receives funds minus our 5% fee
                    let connected_account_id = sess
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("connected_account_id"))
                        .map(String::as_str);
                        
                    if let Some(account_id) = connected_account_id {
                        info!("Payment uses destination charges - Stripe will automatically transfer {} cents to account {}", amount_to_cause, account_id);
                    }
                    
                    // For donations, we need to calculate tokens received based on bonding curve
                    // This will be filled in by the credit_account_with_fee_split response
                    0.0 // Placeholder - actual amount set after token minting
                };

                // Only process if we have a valid wallet address
                if client_ref != "none" && !client_ref.is_empty() {
                    let actual_tokens_received = if is_topup {
                        // For USD topups, credit 1:1 without fees
                        info!("Processing USD topup - no fees applied");
                        webhook_service.credit_account(
                            token_symbol,
                            total,
                            client_ref,
                        ).await?;
                        total as f64
                    } else {
                        // For donations, apply fee split
                        info!("Processing donation - applying 5% platform fee");
                        webhook_service.credit_account_with_fee_split(
                            token_symbol,
                            total,
                            client_ref,
                        ).await?
                    };
                    
                    // Get token image URL
                    let token_image_url = if token_symbol != "USD" && token_symbol != "unknown" {
                        match mongodb_service.get_cause_by_token_symbol(token_symbol).await {
                            Ok(Some(cause)) => cause.token_image_url,
                            _ => None
                        }
                    } else {
                        None // USD deposits don't have an image
                    };
                    
                    // Save deposit record
                    let deposit = DepositRecord {
                        id: None,
                        wallet_address: client_ref.to_string(),
                        token_symbol: token_symbol.to_string(),
                        token_image_url,
                        amount_deposited_usd: amount_usd,
                        amount_tokens_received: actual_tokens_received,
                        created_at: chrono::Utc::now().timestamp(),
                    };
                    
                    if let Err(e) = mongodb_service.save_deposit_record(deposit).await {
                        error!("Failed to save deposit record: {:?}", e);
                        // Don't fail the webhook, just log
                    }
                } else {
                    error!("No wallet address provided for session {}, skipping token distribution", session_id);
                }
            }
        }
        EventType::PaymentIntentSucceeded => {
            if let EventObject::PaymentIntent(pi) = event.data.object {
                info!("received payment_intent.succeeded → {}", pi.id);
                info!("amount: {} {}", pi.amount, pi.currency);
                
                // For now, just log it. You can add token crediting logic here later
            }
        }
        EventType::AccountUpdated => {
            if let EventObject::Account(account) = event.data.object {
                info!("received account.updated for account: {}", account.id);
                info!("  charges_enabled: {:?}", account.charges_enabled);
                info!("  details_submitted: {:?}", account.details_submitted);
                info!("  payouts_enabled: {:?}", account.payouts_enabled);
                
                // Check if onboarding is complete
                if account.charges_enabled.unwrap_or(false) && 
                   account.details_submitted.unwrap_or(false) {
                    
                    info!("Account {} is fully onboarded!", account.id);
                    
                    // Get draft_id from metadata
                    if let Some(metadata) = account.metadata {
                        if let Some(draft_id) = metadata.get("draft_id") {
                            info!("Found draft_id in metadata: {}", draft_id);
                            
                            // Complete cause creation
                            match cause_service.complete_cause_from_draft(draft_id).await {
                                Ok(cause) => {
                                    info!("Successfully created cause from draft: {}", cause.name);
                                },
                                Err(e) => {
                                    error!("Failed to create cause from draft: {:?}", e);
                                    // Don't fail the webhook - we can retry manually
                                }
                            }
                        } else {
                            info!("No draft_id found in metadata for account {}", account.id);
                        }
                    }
                } else {
                    info!("Account {} not fully onboarded yet", account.id);
                }
                
                // Always check for payouts_enabled updates (can happen after onboarding)
                if account.payouts_enabled.unwrap_or(false) {
                    info!("Account {} has payouts_enabled", account.id);
                    
                    // Update any existing causes with this account ID
                    match cause_service.update_causes_payouts_status(&account.id.to_string(), true).await {
                        Ok(count) => {
                            if count > 0 {
                                info!("Updated {} causes with payouts_enabled status", count);
                            }
                        },
                        Err(e) => {
                            error!("Failed to update causes with payouts status: {:?}", e);
                        }
                    }
                }
            }
        }
        other => info!("unhandled stripe event type: {:?}", other),
    }

    Ok(())
}

fn get_header_value<'b>(req: &'b HttpRequest, key: &'b str) -> Option<&'b str> {
    req.headers().get(key)?.to_str().ok()
}
