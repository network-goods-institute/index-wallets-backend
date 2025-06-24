use actix_web::web;
use crate::handlers::webhook_handlers::handle_stripe_webhook;
use crate::handlers::purchase_webhook_handlers::handle_stripe_purchases_webhook;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/webhooks")
            .route("/stripe", web::post().to(handle_stripe_webhook))
            .route("/purchases", web::post().to(handle_stripe_purchases_webhook))
    );
}
