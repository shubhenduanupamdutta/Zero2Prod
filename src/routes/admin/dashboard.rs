use actix_web::{HttpResponse, http::header::ContentType, web};
use anyhow::Context as _;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{routes::reject_anonymous_users, session_state::TypedSession, utils::e500};


pub async fn admin_dashboard(
    session: TypedSession,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = reject_anonymous_users(session)?;
    let username = get_username(user_id, &pool).await.map_err(e500)?;
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
    <h3>Welcome {username}!</h3>
    <p>Available actions:</p>
    <ol>
        <li><a href="/admin/password">Change password</a></li>
        <li>
            <form action="/admin/logout" method="post">
                <button type="submit">Logout</button>
            </form>
        </li>
    </ol>
</body>
</html>"#
        )))
}


#[tracing::instrument(name = "Get username", skip(pool))]
pub async fn get_username(user_id: Uuid, pool: &PgPool) -> Result<String, anyhow::Error> {
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
