use actix_web::web;
use crate::handlers::vault_handler;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/vault")
            .route("/vaults/{pubkey}", web::get().to(vault_handler::get_vault))
            .route("/signed-verifiable", web::post().to(vault_handler::post_signed_verifiable))
            .route("/execute", web::post().to(vault_handler::post_execute))
            .route("/submit-proof", web::post().to(vault_handler::post_submit_proof))
    );
}
