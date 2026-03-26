use actix_web::{HttpResponse, web};
use actix_web_flash_messages::FlashMessage;
use anyhow::Context as _;
use serde::Deserialize;
use sqlx::PgPool;

use crate::{
    authentication::UserId,
    domain::NewSubscriber,
    email_client::EmailClient,
    utils::{e500, see_other},
};

#[derive(Deserialize)]
pub struct BodyData {
    title: String,
    content: String,
}

#[tracing::instrument(name = "Publish a newsletter issue",
    skip(body, pool, email_client),
    fields(user_id=%*user_id)
)]
pub async fn publish_newsletter(
    body: web::Form<BodyData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    user_id: web::ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let subscribers = get_confirmed_subscribers(&pool)
        .await
        .context("Failed to get confirmed subscribers")
        .map_err(e500)?;

    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &subscriber.name,
                        &body.title,
                        &body.content,
                    )
                    .await
                    .with_context(|| {
                        format!("Failed to send newsletter email to {}", subscriber.email)
                    })
                    .map_err(e500)?;
            },
            Err(error) => {
                tracing::warn!(
                    error.cause_chain = ?error,
                    error.message = %error,
                    "Skipping a confirmed subscriber because their stored contact details are invalid"
                );
            },
        }
    }

    FlashMessage::info("The newsletter issue has been published!").send();
    Ok(see_other("/admin/newsletters"))
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pool))]
async fn get_confirmed_subscribers(
    pool: &PgPool,
) -> Result<Vec<Result<NewSubscriber, anyhow::Error>>, sqlx::Error> {
    let confirmed_subscribers = sqlx::query!(
        r#"
        SELECT email, name
        FROM subscriptions
        WHERE status = 'confirmed'
        "#
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| NewSubscriber::parse(row.name, row.email))
    .collect();

    Ok(confirmed_subscribers)
}
