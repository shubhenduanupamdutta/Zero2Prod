use actix_web::{web, HttpResponse};
use serde::Deserialize;

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct FormData {
    name: String,
    email: String,
}

pub async fn subscribe(_form: web::Form<FormData>) -> HttpResponse {
    HttpResponse::Ok().finish()
}
