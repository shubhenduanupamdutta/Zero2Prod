use std::fmt;

use actix_web::{HttpResponse, ResponseError, http::StatusCode, web};
use anyhow::Context as _;
use serde::Deserialize;
use sqlx::PgPool;

use crate::{domain::NewSubscriber, email_client::EmailClient, utils::error_chain_fmt};

#[derive(Deserialize)]
pub struct BodyData {
    title: String,
    content: Content,
}

#[derive(Deserialize)]
pub struct Content {
    html: String,
}

#[derive(thiserror::Error)]
pub enum PublishError {
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl fmt::Debug for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for PublishError {
    fn status_code(&self) -> StatusCode {
        match self {
            PublishError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

pub async fn publish_newsletter(
    body: web::Json<BodyData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> Result<HttpResponse, PublishError> {
    let subscribers = get_confirmed_subscribers(&pool)
        .await
        .context("Failed to get confirmed subscribers")?;

    for subscriber in subscribers {
        email_client
            .send_email(
                subscriber.email.clone(),
                subscriber.name,
                &body.title,
                &body.content.html,
            )
            .await
            .with_context(|| format!("Failed to send newsletter email to {}", subscriber.email))?;
    }
    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pool))]
async fn get_confirmed_subscribers(pool: &PgPool) -> Result<Vec<NewSubscriber>, sqlx::Error> {
    struct Row {
        email: String,
        name: String,
    }

    let rows = sqlx::query_as!(
        Row,
        r#"
        SELECT email, name
        FROM subscriptions
        WHERE status = 'confirmed'
        "#
    )
    .fetch_all(pool)
    .await?;

    let confirmed_subscribers = rows
        .into_iter()
        .filter_map(|r| {
            match NewSubscriber::parse(r.name, r.email) {
                Ok(subscriber) => Some(subscriber),
                Err(e) => {
                    tracing::warn!(
                        "A confirmed subscriber is using an invalid email address: \n{}",
                        e
                    );
                    None
                },
            }
        })
        .collect();
    Ok(confirmed_subscribers)
}
