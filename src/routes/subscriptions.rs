use std::{error, fmt};

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::{EmailClient, EmailTemplateEngine},
    startup::ApplicationBaseUrl,
};
use actix_web::http::StatusCode;
use actix_web::{HttpResponse, ResponseError, web};
use chrono::Utc;
use rand::{Rng, distr::Alphanumeric, rng};
use serde::Deserialize;
use sqlx::{Executor, PgPool, Postgres, Transaction};
use tracing::error;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct FormData {
    name: String,
    email: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;
        Ok(NewSubscriber {
            name,
            email,
        })
    }
}

pub struct StoreTokenError(sqlx::Error);

impl fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "A database error was encountered while trying to store a subscription token."
        )
    }
}

fn error_chain_fmt(e: &impl error::Error, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "{}", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "\nCaused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}

impl fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // The compiler thransparently casts `&sqlx::Error` into a `&dyn Error`.
        Some(&self.0)
    }
}

pub enum SubscribeError {
    ValidationError(String),
    StoreTokenError(StoreTokenError),
    SendEmailError(reqwest::Error),
    PoolError(sqlx::Error),
    InsertSubscriberError(sqlx::Error),
    TransactionCommitError(sqlx::Error),
}

impl fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl fmt::Display for SubscribeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubscribeError::ValidationError(e) => write!(f, "{}", e),
            SubscribeError::StoreTokenError(_) => {
                write!(f, "Failed to store the subscription token in the database.")
            },
            SubscribeError::SendEmailError(_) => write!(f, "Failed to send a confirmation email"),
            SubscribeError::PoolError(_) => {
                write!(f, "Failed to acquire a Postgres connection from the pool.")
            },
            SubscribeError::InsertSubscriberError(_) => {
                write!(f, "Failedto insert new subscriber details in the database.")
            },
            SubscribeError::TransactionCommitError(_) => write!(
                f,
                "Failed to commit SQL transaction to store a new subscriber."
            ),
        }
    }
}

impl error::Error for SubscribeError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            // We consider &str as root cause of the error, so we don't return it as a source.
            SubscribeError::ValidationError(_) => None,
            SubscribeError::StoreTokenError(e) => Some(e),
            SubscribeError::SendEmailError(e) => Some(e),
            SubscribeError::PoolError(e) => Some(e),
            SubscribeError::InsertSubscriberError(e) => Some(e),
            SubscribeError::TransactionCommitError(e) => Some(e),
        }
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<reqwest::Error> for SubscribeError {
    fn from(value: reqwest::Error) -> Self {
        Self::SendEmailError(value)
    }
}

impl From<StoreTokenError> for SubscribeError {
    fn from(value: StoreTokenError) -> Self {
        Self::StoreTokenError(value)
    }
}

impl From<String> for SubscribeError {
    fn from(value: String) -> Self {
        Self::ValidationError(value)
    }
}

/// Generate a random 25-character-long case sensitive subscription token
fn generate_subscription_token() -> String {
    let mut rng = rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

#[tracing::instrument(name="Adding a new subscriber", skip(form, pool, email_client, base_url, template_engine),
 fields(
    subscriber_email=%form.email,
    subscriber_name=%form.name
))]
pub async fn subscribe(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
    template_engine: web::Data<EmailTemplateEngine>,
) -> Result<HttpResponse, SubscribeError> {
    let new_subscriber = form.0.try_into()?;
    let mut transaction = pool.begin().await.map_err(SubscribeError::PoolError)?;

    let (subscriber_id, status) = insert_subscriber(&mut transaction, &new_subscriber)
        .await
        .map_err(SubscribeError::InsertSubscriberError)?;

    if status == "confirmed" {
        if transaction.commit().await.is_err() {
            return Ok(HttpResponse::InternalServerError().finish());
        }
        send_reminder_email(&email_client, new_subscriber, &template_engine).await?;
        return Ok(HttpResponse::Ok().finish());
    };

    let subscription_token = generate_subscription_token();
    store_token(&mut transaction, subscriber_id, &subscription_token).await?;

    transaction
        .commit()
        .await
        .map_err(SubscribeError::TransactionCommitError)?;

    send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
        &template_engine,
    )
    .await?;
    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(
    name = "Store subscription token in the database",
    skip(transaction, subscription_token)
)]
async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    let query = sqlx::query!(
        r#"
        INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    );
    transaction.execute(query).await.map_err(|e| {
        error!("Failed to execute query: {:?}", e);
        StoreTokenError(e)
    })?;
    Ok(())
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(
        email_client,
        new_subscriber,
        base_url,
        subscription_token,
        email_template_engine
    )
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
    email_template_engine: &EmailTemplateEngine,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );
    let html_body = email_template_engine
        .render_confirmation_email(new_subscriber.name.as_ref(), &confirmation_link)
        .unwrap_or_else(|_| {
            format!(
                "Welcome to our newsletter!<br />Click <a href=\"{}\">here</a> to confirm your subscription.",
                confirmation_link
            )
        });

    email_client
        .send_email(
            new_subscriber.email,
            new_subscriber.name,
            "Welcome!",
            &html_body,
        )
        .await
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(new_subscriber, transaction)
)]
async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<(Uuid, String), sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    let result = sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email
        RETURNING id, status
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now()
    )
    .fetch_one(transaction.as_mut())
    .await
    .map_err(|e| {
        error!("Failed to execute query: {:?}", e);
        e
    })?;

    Ok((result.id, result.status))
}

#[tracing::instrument(
    name = "Send a reminder email to an already confirmed subscriber",
    skip(email_client, new_subscriber, email_template_engine)
)]
pub async fn send_reminder_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    email_template_engine: &EmailTemplateEngine,
) -> Result<(), reqwest::Error> {
    let html_body = email_template_engine
        .render_already_subscribed_email(new_subscriber.name.as_ref())
        .unwrap_or_else(|_| {
            "Welcome back to our newsletter!<br />You are already subscribed.".to_string()
        });

    email_client
        .send_email(
            new_subscriber.email,
            new_subscriber.name,
            "You are already subscribed!",
            &html_body,
        )
        .await
}
