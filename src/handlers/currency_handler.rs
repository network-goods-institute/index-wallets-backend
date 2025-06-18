use actix_web::{web, HttpRequest, HttpResponse, error::ErrorInternalServerError};
use log::{info, error};
use std::sync::Mutex;
use delta_executor_sdk::{
    self,
    base::{
        core::Shard,
        crypto::{PrivKey, SignedMessage},
        vaults::{VaultId, TokenMetadata},
        verifiable::{
            token_mint::{TokenMint, TokenSupplyOperation},
            VerifiableType,
        },
    },
    execution::FullDebitExecutor,
    prover::mock::MockProver,
};
use stripe::{EventObject, EventType, Webhook, WebhookError};
use crate::AppState;

type Buffer = Mutex<Vec<VerifiableType>>;

pub async fn mint_base_currency(
    state: web::Data<AppState>
) -> HttpResponse {
    info!("Starting mint_base_currency");
    let issuer_shard = state.shard;
    let token_issuer = VaultId::new(state.keypair.pub_key(), issuer_shard);
    let token_supply = 1000000000;
    let credited = VaultId::new(state.vault_key_pair.pub_key(), issuer_shard);

    // initial nonce for nonexistent vault: 
    let new_nonce = 1; 

    // MintToken issuer: 
    info!("Token issuer: {:?}", token_issuer);

    // Vault that holds the tokens: 
    info!("Credited vault: {:?}", credited);

    let metadata = TokenMetadata {
        name: "USD".to_string(),
        symbol: "USD".to_string(),
    };
    let payload = TokenMint {
        operation: TokenSupplyOperation::Create {
            metadata,
            credited: vec![(credited, token_supply)],
        },
        debited: token_issuer,
        new_nonce, 
    };

    info!("Signing payload");
    let signed = match SignedMessage::sign(payload, &state.keypair) {
        Ok(signed) => signed,
        Err(e) => {
            error!("Failed to sign message: {:?}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };
    
    info!("Pushing to buffer");
    // Handle buffer operations
    if let Err(e) = buffer.lock().map(|mut b| b.push(VerifiableType::TokenMint(signed))) {
        error!("Failed to push to buffer: {:?}", e);
        return HttpResponse::InternalServerError().finish();
    }

    info!("Draining buffer");
    let messages = match buffer.lock() {
        Ok(mut guard) => guard.drain(..).collect(),
        Err(e) => {
            error!("Failed to lock buffer for draining: {:?}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    info!("Messages: {:?}", messages);

    info!("Executing and proving");
    match runtime.execute_submit_prove(messages).await {
        Ok(Some(_)) => {
            info!("Successfully minted currency");
            HttpResponse::Ok().finish()
        },
        Ok(None) => {
            error!("Execute returned None");
            HttpResponse::InternalServerError().finish()
        },
        Err(e) => {
            error!("Failed to execute and prove: {:?}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

pub async fn webhook_handler(req: HttpRequest, payload: web::Bytes) -> HttpResponse {
    handle_webhook(&req, &payload).unwrap_or_else(|_| {
        error!("Failed to handle webhook");
        HttpResponse::InternalServerError().finish();
    });
    HttpResponse::Ok().finish()
}

fn handle_webhook(req: &HttpRequest, payload: &web::Bytes) -> Result<(), WebhookError> {
    // 1. get it as str
    let payload_str = std::str::from_utf8(payload.as_ref()).unwrap();
    // 2. grab the signature header
    let stripe_signature = get_header_value(&req, "Stripe-Signature").unwrap_or_default();

    // 3. your webhook secret
    let secret = "whsec_865917b1ca5cef5a6eea9cd086fb15fd7da35ddcece4c13db311d9f71d1fdc25"; 
    // 4. verify & parse
    let event = Webhook::construct_event(payload_str, stripe_signature, &secret)?;

    // 5. dispatch based on the event type
    match event.type_ {
        EventType::AccountUpdated => {
            if let EventObject::Account(acc) = event.data.object {
                info!("received account.updated → {}", acc.id);
                // … your AccountUpdated logic …
            }
        }
        EventType::CheckoutSessionCompleted => {
            if let EventObject::CheckoutSession(sess) = event.data.object {
                let session_id = &sess.id;

                // client_reference_id is Option<String>
                let client_ref = sess
                    .client_reference_id
                    .as_deref()
                    .unwrap_or("none");
        
                // amount_total is Option<i64>
                let total = sess
                    .amount_total
                    .unwrap_or(0);
        
                // metadata is Option<HashMap<String,String>>
                let token = sess
                    .metadata
                    .as_ref()                     // &Metadata
                    .and_then(|m| m.get("TokenName"))
                    .map(String::as_str)
                    .unwrap_or("unknown");
        
                info!("received checkout.session.completed → {}", session_id);
                info!("from id: {}", client_ref);
                info!("for amount: {}", total);
                info!("for token: {}", token); 
            }
        }
        other => info!("unhandled stripe event type: {:?}", other),
    }

    Ok(())
}

fn get_header_value<'b>(req: &'b HttpRequest, key: &'b str) -> Option<&'b str> {
    req.headers().get(key)?.to_str().ok()
}
