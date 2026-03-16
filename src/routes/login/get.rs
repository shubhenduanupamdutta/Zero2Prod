use actix_web::{HttpResponse, http::header::ContentType, web};
use hmac::{Hmac, Mac};
use secrecy::ExposeSecret;

use crate::startup::HmacSecret;

#[derive(serde::Deserialize)]
pub struct QueryParams {
    error: String,
    tag: String,
}

impl QueryParams {
    fn verify(self, secret: &HmacSecret) -> Result<String, anyhow::Error> {
        let tag = hex::decode(self.tag)?;
        let query_string = format!("error={}", urlencoding::Encoded::new(&self.error));
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.0.expose_secret().as_bytes())?;
        mac.update(query_string.as_bytes());
        mac.verify_slice(&tag)?;
        Ok(self.error)
    }
}

pub async fn login_form(
    query: Option<web::Query<QueryParams>>,
    secret: web::Data<HmacSecret>,
) -> HttpResponse {
    let error_html = match query {
        None => "".into(),
        Some(query) => match query.0.verify(&secret) {
            Ok(error_message) => {
                format!(
                    r#"<p style="color: red;">{}</p>"#,
                    htmlescape::encode_minimal(&error_message)
                )
            },
            Err(e) => {
                tracing::warn!(
                    error.message = %e,
                    error.cause_chain = ?e,
                    "Failed to verify the HMAC tag for the error message"
                );
                "".into()
            },
        },
    };
    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=UTF-8">
    <title>Login</title>
</head>
<body>
    {error_html}
    <form action="/login" method="post">
        <label>Username
            <input type="text" name="username" placeholder="Enter username" required>
        </label>
        <label>Password
            <input type="password" name="password" placeholder="Enter password" required>
        </label>
        <button type="submit">Login</button>
    </form>
</body>
</html>"#
        ))
}
