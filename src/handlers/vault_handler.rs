use actix_web::{
    error::ErrorInternalServerError,
    web,
    HttpResponse,
    Responder,
};
use delta_executor_sdk::{
    self,
    base::{
        crypto::{
            HashDigest,
            Ed25519PubKey,
        },
        verifiable::VerifiableType,
    },
    execution::FullDebitExecutor,
    proving
};
use std::sync::Mutex;

type Buffer = Mutex<Vec<VerifiableType>>;
type Runtime = delta_executor_sdk::Runtime<FullDebitExecutor, proving::mock::Client>;

pub async fn get_vault(key: web::Path<Ed25519PubKey>, runtime: web::Data<Runtime>) -> HttpResponse {
    match runtime.get_vault(&key.into_inner()) {
        Ok(Some(vault)) => HttpResponse::Ok().json(vault),
        Ok(None) => HttpResponse::NotFound().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

pub async fn post_signed_verifiable(
    request: web::Json<VerifiableType>,
    buffer: web::Data<Buffer>,
) -> actix_web::Result<HttpResponse> {
    buffer
        .lock()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?
        .push(request.into_inner());

    Ok(HttpResponse::Ok().finish())
}

pub async fn post_execute(
    runtime: web::Data<Runtime>,
    buffer: web::Data<Buffer>,
) -> actix_web::Result<impl Responder> {
    let messages = buffer
        .lock()
        .map_err(|e| ErrorInternalServerError(e.to_string()))?
        .drain(..)
        .collect();

    let sdl = runtime
        .execute_submit_prove(messages)
        .await
        .map_err(ErrorInternalServerError)?;

    Ok(web::Json(sdl))
}

pub async fn post_submit_proof(
    request: web::Json<HashDigest>,
    runtime: web::Data<Runtime>,
) -> actix_web::Result<impl Responder> {
    runtime
        .submit_proof(request.into_inner())
        .await
        .map_err(ErrorInternalServerError)?;

    Ok(HttpResponse::Ok().finish())
}
