use actix_web::{web, HttpRequest, HttpResponse};
use crate::handlers::webhook_handlers::handle_stripe_webhook;

async fn test_webhook() -> actix_web::HttpResponse {
    log::info!("TEST WEBHOOK HIT!");
    actix_web::HttpResponse::Ok().body("Test webhook works")
}

async fn stripe_webhook_minimal(
    req: HttpRequest,
    payload: web::Bytes,
) -> HttpResponse {
    log::info!("=== MINIMAL STRIPE WEBHOOK HIT ===");
    log::info!("Headers: {:?}", req.headers());
    log::info!("Payload size: {} bytes", payload.len());
    HttpResponse::Ok().body("Minimal webhook works")
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/webhooks")
            .route("/stripe", web::post().to(handle_stripe_webhook))
            .route("/stripe-minimal", web::post().to(stripe_webhook_minimal))
            .route("/test", web::post().to(test_webhook))
    );
}
