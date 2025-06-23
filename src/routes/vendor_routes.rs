use actix_web::web;
use crate::handlers::vendor_handlers;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/vendors")
            .route("/partnered", web::get().to(vendor_handlers::get_partnered_vendors))
    );
}