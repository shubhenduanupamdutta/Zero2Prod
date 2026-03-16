use actix_web::{HttpResponse, http::header::LOCATION, web};
use secrecy::SecretString;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct FormData {
    username: String,
    password: SecretString,
}

pub async fn login(_form: web::Form<FormData>) -> HttpResponse {
    HttpResponse::SeeOther()
        .insert_header((LOCATION, "/"))
        .finish()
}
