use actix_web::web;
use crate::handlers::token_handler;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/tokens")
            .route("", web::get().to(token_handler::get_all_tokens))
            .route("/{name}", web::get().to(token_handler::get_token_by_name))
    );
}