use actix_web::{web, HttpResponse};
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use tracing::{error, info};
use uuid::Uuid;

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct FormData {
    name: String,
    email: String,
}

pub async fn subscribe(form: web::Form<FormData>, pool: web::Data<PgPool>) -> HttpResponse {
    let request_id = Uuid::new_v4();
    info!(
        "request_id {} - Adding '{}' '{}' as a new subscriber.",
        request_id, form.name, form.email
    );

    info!(
        "request_id {} - Saving new subscriber details in the database.",
        request_id
    );
    match sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        form.email,
        form.name,
        Utc::now()
    )
    .execute(pool.get_ref())
    .await
    {
        Ok(_) => {
            info!(
                "request_id {} - New subscriber details have been saved successfully.",
                request_id
            );
            HttpResponse::Ok().finish()
        },
        Err(e) => {
            error!(
                "request_id {} - Failed to execute query: {:?}",
                request_id, e
            );
            HttpResponse::InternalServerError().finish()
        },
    }
}
