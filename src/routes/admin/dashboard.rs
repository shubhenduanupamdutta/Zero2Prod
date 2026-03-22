use std::fmt;

use crate::session_state::TypedSession;
use actix_web::{HttpResponse, http::header::ContentType, web};
use anyhow::Context as _;
use sqlx::PgPool;
use uuid::Uuid;

/// Return an opaque 500 while preserving the error's root cause for logging
fn e500<T>(e: T) -> actix_web::Error
where
    T: fmt::Debug + fmt::Display + 'static,
{
    actix_web::error::ErrorInternalServerError(e)
}


pub async fn admin_dashboard(
    session: TypedSession,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let username = if let Some(user_id) = session.get_user_id().map_err(e500)? {
        get_username(user_id, &pool).await.map_err(e500)?
    } else {
        todo!()
    };

    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=utf-8">
    <title>Admin Dashboard</title>
</head>
<body>
    <h1>Welcome {username}!</h1>
</body>
</html>"#
        )))
}


#[tracing::instrument(name = "Get username", skip(pool))]
async fn get_username(user_id: Uuid, pool: &PgPool) -> Result<String, anyhow::Error> {
    let row = sqlx::query!(
        r#"
        SELECT username
        FROM users
        WHERE user_id = $1
        "#,
        user_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to perform a query to retrieve a username.")?;

    Ok(row.username)
}
