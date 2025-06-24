use actix_web::{web, HttpRequest, HttpResponse};
use log::{info, error};
use stripe::{Webhook, EventObject, EventType};

use crate::services::{WebhookService, CauseService};
use crate::models::WebhookError;

pub async fn handle_stripe_webhook(
    req: HttpRequest,
    payload: web::Bytes,
    webhook_service: web::Data<WebhookService>,
    cause_service: web::Data<CauseService>,
) -> HttpResponse {
    info!("=== STRIPE CONNECT WEBHOOK RECEIVED ===");
    match process_stripe_webhook(&req, &payload, webhook_service, cause_service).await {
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
) -> Result<(), WebhookError> {
    let payload_str = std::str::from_utf8(payload.as_ref())
        .map_err(|e| WebhookError::InvalidPayload(e.to_string()))?;

    let stripe_signature = get_header_value(&req, "Stripe-Signature")
        .ok_or_else(|| WebhookError::MissingSignature)?;

    let event = Webhook::construct_event(
        payload_str,
        stripe_signature,
        webhook_service.get_stripe_secret(),
    )?;

    match event.type_ {
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
        other => info!("unhandled stripe connect event type: {:?}", other),
    }

    Ok(())
}

fn get_header_value<'b>(req: &'b HttpRequest, key: &'b str) -> Option<&'b str> {
    req.headers().get(key)?.to_str().ok()
}
