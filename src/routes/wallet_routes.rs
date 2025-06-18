use actix_web::web;
use crate::handlers::wallet_handlers;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/wallet")
        // TODO: make routes more consistent (e.g. balances/{wallet_address})
            .route("/{wallet_address}", web::get().to(wallet_handlers::get_vault))
            .route("/{wallet_address}/balances", web::get().to(wallet_handlers::get_user_balances))
            .route("/{wallet_address}/valuations", web::get().to(wallet_handlers::get_user_valuations))
            .route("/{wallet_address}/valuations", web::post().to(wallet_handlers::update_user_valuation))
    );
}