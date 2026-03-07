use actix_web::{web, HttpResponse};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::error;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct Parameters {
    subscription_token: String,
}

struct TokenRow {
    subscriber_id: Uuid,
    created_at: DateTime<Utc>,
    consumed_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct ConfirmationResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[tracing::instrument(name = "Confirm a pending subscriber", skip(parameters, pool))]
pub async fn confirm(
    parameters: web::Query<Parameters>,
    pool: web::Data<PgPool>,
    expiry_seconds: web::Data<u32>,
) -> HttpResponse {
    let record = match get_token_row(&pool, &parameters.subscription_token).await {
        Ok(record) => record,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let row = match record {
        None => return HttpResponse::Unauthorized().finish(),
        Some(row) => row,
    };

    if row.consumed_at.is_some() {
        return HttpResponse::Ok().json(ConfirmationResponse {
            status: "already_confirmed".to_string(),
            message: Some("This subscription has already been confirmed.".to_string()),
        });
    }

    if is_any_token_consumed(&pool, row.subscriber_id)
        .await
        .unwrap_or(false)
    {
        if mark_token_as_consumed(&pool, &parameters.subscription_token)
            .await
            .is_err()
        {
            return HttpResponse::InternalServerError().finish();
        }
        return HttpResponse::Ok().json(ConfirmationResponse {
            status: "already_confirmed".to_string(),
            message: Some("This subscription has already been confirmed.".to_string()),
        });
    }

    let now = Utc::now();
    if now - row.created_at > chrono::Duration::seconds(*expiry_seconds.into_inner() as i64) {
        return HttpResponse::Unauthorized().finish();
    };

    let subscriber_was_confirmed = match confirm_subscriber(&pool, row.subscriber_id).await {
        Ok(was_confirmed) => was_confirmed,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    if subscriber_was_confirmed {
        return HttpResponse::Ok().json(ConfirmationResponse {
            status: "already_confirmed".to_string(),
            message: Some("This subscription has already been confirmed.".to_string()),
        });
    } else {
        if mark_token_as_consumed(&pool, &parameters.subscription_token)
            .await
            .is_err()
        {
            return HttpResponse::InternalServerError().finish();
        }
        return HttpResponse::Ok().json(ConfirmationResponse {
            status: "confirmed".to_string(),
            message: None,
        });
    }
}

#[tracing::instrument(name = "Mark subscriber as confirmed", skip(pool, subscriber_id))]
pub async fn confirm_subscriber(pool: &PgPool, subscriber_id: Uuid) -> Result<bool, sqlx::Error> {
    let record = sqlx::query!(
        r#"
        WITH prev AS (
            SELECT status FROM subscriptions WHERE id = $1
        )
        UPDATE subscriptions
        SET status = 'confirmed'
        WHERE id = $1
        RETURNING (SELECT status = 'confirmed' FROM prev) AS "was_already_confirmed!"
        "#,
        subscriber_id
    )
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(record.was_already_confirmed)
}

#[tracing::instrument(
    name = "Get subscription token details",
    skip(pool, subscription_token)
)]
async fn get_token_row(
    pool: &PgPool,
    subscription_token: &str,
) -> Result<Option<TokenRow>, sqlx::Error> {
    sqlx::query_as!(
        TokenRow,
        r#"SELECT subscriber_id, created_at, consumed_at
            FROM subscription_tokens WHERE subscription_token = $1
        "#,
        subscription_token
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| {
        error!("Failed to execute query: {:?}", e);
        e
    })
}

#[tracing::instrument(
    name = "Check if any row with user id is consumed",
    skip(pool, subscriber_id)
)]
pub async fn is_any_token_consumed(
    pool: &PgPool,
    subscriber_id: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query!(
        r#"SELECT consumed_at FROM subscription_tokens WHERE subscriber_id = $1"#,
        subscriber_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!("Failed to execute query: {:?}", e);
        e
    })
    .map(|rows| rows.into_iter().any(|r| r.consumed_at.is_some()))
}

#[tracing::instrument(name = "Mark token as consumed", skip(pool, subscription_token))]
pub async fn mark_token_as_consumed(
    pool: &PgPool,
    subscription_token: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE subscription_tokens SET consumed_at = $1 WHERE subscription_token = $2"#,
        Utc::now(),
        subscription_token
    )
    .execute(pool)
    .await
    .map_err(|e| {
        error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(())
}
