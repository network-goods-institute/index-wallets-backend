mod message_routes;
mod vault_routes;
mod cause_routes;
mod webhook_routes;
mod wallet_routes;
mod vendor_routes;

pub use message_routes::message_routes::configure as configure_message_routes;
pub use vault_routes::configure as configure_vault_routes;
pub use cause_routes::configure as configure_cause_routes;
pub use webhook_routes::configure as configure_webhook_routes;
pub use wallet_routes::configure as configure_wallet_routes;
pub use vendor_routes::configure as configure_vendor_routes;

pub fn configure(cfg: &mut actix_web::web::ServiceConfig) {
    configure_message_routes(cfg);
    configure_vault_routes(cfg);
    configure_cause_routes(cfg);
    configure_webhook_routes(cfg);
    configure_wallet_routes(cfg);
    configure_vendor_routes(cfg);
}