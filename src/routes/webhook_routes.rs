use actix_web::web;
use crate::handlers::webhook_handlers::handle_stripe_webhook;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/webhooks")
            .route("/stripe", web::post().to(handle_stripe_webhook))
    );
}
