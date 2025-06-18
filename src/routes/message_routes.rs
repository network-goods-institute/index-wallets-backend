pub mod message_routes {
    use actix_web::web;
    use crate::handlers;
    use log::{info, error};
    use actix_web::middleware::Logger;

    pub fn configure(cfg: &mut web::ServiceConfig) {
        cfg.service(
            web::scope("/api")
                .wrap(Logger::default())
                .route("/", web::get().to(handlers::hello))
                .route("/health", web::get().to(handlers::health_check))
                .route("/echo", web::post().to(handlers::echo))
                .route("/users", web::post().to(handlers::create_user))
                .route("/users/{wallet_address}", web::get().to(handlers::get_user))

                // Payment routes for creation, supplementation/calculation, and status, abstract this later into 
                // own routes: 

                .route("/payments", web::post().to(handlers::create_payment))
                .route("/payments/{payment_id}/supplement", web::post().to(handlers::supplement_transaction))
                .route("/payments/{payment_id}/status", web::get().to(handlers::get_payment_status))
                .route("/payments/{payment_id}/sign", web::post().to(handlers::process_signed_transaction))
                .route("/payments/{payment_id}", web::delete().to(handlers::delete_payment))
                
                // Transaction history route
                .route("/users/{user_address}/transactions", web::get().to(handlers::get_user_transaction_history))
        );
    }
} 