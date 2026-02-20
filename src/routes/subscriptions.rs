use actix_web::{web, HttpResponse};
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;
use tracing::{Instrument, error, info, info_span};
use uuid::Uuid;

#[allow(dead_code)]
#[derive(Deserialize)]
pub struct FormData {
    name: String,
    email: String,
}

pub async fn subscribe(form: web::Form<FormData>, pool: web::Data<PgPool>) -> HttpResponse {
    let request_id = Uuid::new_v4();
    let request_span = info_span!("Adding a new subscriber", %request_id, subscriber_email = %form.email, subscriber_name=%form.name);
    let _request_span_guard = request_span.enter();

    let query_span = tracing::info_span!("Saving new subscriber details in the database");

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
    .instrument(query_span)
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
