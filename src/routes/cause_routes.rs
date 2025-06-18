use actix_web::web;
use crate::handlers::cause_handlers;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/causes")
            .route("", web::post().to(cause_handlers::create_cause))
            .route("", web::get().to(cause_handlers::get_all_causes))
            .route("/featured", web::get().to(cause_handlers::get_featured_causes))
            .route("/admin/all", web::get().to(cause_handlers::get_all_causes_admin))
            .route("/by-token/{token_name}", web::get().to(cause_handlers::get_cause_by_token_name))
            .route("/by-name/{name}", web::get().to(cause_handlers::get_cause_by_name))
            .route("/by-symbol/{token_symbol}", web::get().to(cause_handlers::get_cause_by_token_symbol))
            .route("/drafts/find", web::post().to(cause_handlers::find_drafts_by_email))
            .route("/drafts/{draft_id}/status", web::get().to(cause_handlers::get_draft_status))
            .route("/donate", web::post().to(cause_handlers::create_donation_session))
            .route("/validate/name", web::post().to(cause_handlers::validate_cause_name))
            .route("/validate/token-symbol", web::post().to(cause_handlers::validate_token_symbol))
            .route("/validate/token-name", web::post().to(cause_handlers::validate_token_name))
            .route("/{id}", web::get().to(cause_handlers::get_cause))
            .route("/{id}", web::put().to(cause_handlers::update_cause))
            .route("/{id}", web::delete().to(cause_handlers::delete_cause))
            .route("/{id}/onboarding", web::get().to(cause_handlers::get_onboarding_link))
            .route("/{id}/status", web::get().to(cause_handlers::check_account_status))
    );
}