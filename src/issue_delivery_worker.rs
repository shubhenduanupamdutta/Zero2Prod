use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use sqlx::{Executor, PgPool, Postgres, Transaction};
use tracing::{Span, field::display};
use uuid::Uuid;

use crate::{
    configuration::Settings,
    domain::NewSubscriber,
    email_client::EmailClient,
    startup::get_connection_pool,
};


const MAX_RETRIES: i16 = 5;

#[tracing::instrument(
    skip_all,
    fields(
        newsletter_issue_id=tracing::field::Empty,
        subscriber_email=tracing::field::Empty,
    ),
    err
)]


async fn try_execute_task(
    pool: &PgPool,
    email_client: &EmailClient,
) -> Result<ExecutionOutcome, anyhow::Error> {
    let task = dequeue_task(pool).await?;
    if task.is_none() {
        return Ok(ExecutionOutcome::EmptyQueue);
    }
    let (transaction, issue_id, email, name, retries) = task.unwrap();
    Span::current()
        .record("newsletter_issue_id", display(issue_id))
        .record("subscriber_email", display(&email));

    match NewSubscriber::parse(email.clone(), name) {
        Ok(recipient) => {
            let issue = get_issue(pool, issue_id).await?;
            if let Err(e) = email_client
                .send_email(
                    &recipient.email,
                    &recipient.name,
                    &issue.title,
                    &issue.content,
                )
                .await
            {
                if retries < MAX_RETRIES {
                    // Store for retry with an exponential backoff and warn in log
                    tracing::warn!(
                        retries,
                        max_retries = MAX_RETRIES,
                        "Email delivery failed, Updating task to retry later {}/{} for {}",
                        retries + 1,
                        MAX_RETRIES,
                        recipient.email
                    );
                    store_for_retry(transaction, issue_id, &email, retries).await?;
                    return Ok(ExecutionOutcome::TaskCompleted);
                }
                tracing::error!(
                    error.cause_chain = ?e,
                    error.message = %e,
                    "Permanent failure when sending newsletter issue to {}, reached max retries {}/{}",
                    recipient.email, retries, MAX_RETRIES
                );
            }
        },
        Err(e) => {
            tracing::error!(
                error.cause_chain = ?e,
                error.message = %e,
                "Skipping a confirmed subscriber. Their stored contact details are invalid. Subscriber email: {}",
                email
            );
        },
    }

    delete_task(transaction, issue_id, &email).await?;


    Ok(ExecutionOutcome::TaskCompleted)
}

type PgTransaction = Transaction<'static, Postgres>;

/// Dequeue a delivery task from the queue, returning the transaction that locks the selected task,
/// the newsletter issue id, and the subscriber email and name
///
/// # Args
///
/// * `pool` - The connection pool to the Postgres database
///
/// # Returns
/// * `Ok(Some((transaction, issue_id, email, name, retries)))` - If a task is available and has
///   been locked for processing
/// * `Ok(None)` - If no task is available for processing
async fn dequeue_task(
    pool: &PgPool,
) -> Result<Option<(PgTransaction, Uuid, String, String, i16)>, anyhow::Error> {
    // We need to start a transaction to be able to lock the selected task
    let mut transaction = pool.begin().await?;
    let record = sqlx::query!(
        r#"
        SELECT q.newsletter_issue_id, q.subscriber_email, s.name AS subscriber_name, q.n_retries
        FROM issue_delivery_queue q
        JOIN subscriptions s ON s.email = q.subscriber_email
        WHERE q.execute_after <= now()
        FOR UPDATE OF q
        SKIP LOCKED
        LIMIT 1
        "#,
    )
    .fetch_optional(&mut *transaction)
    .await?;

    if let Some(r) = record {
        Ok(Some((
            transaction,
            r.newsletter_issue_id,
            r.subscriber_email,
            r.subscriber_name,
            r.n_retries,
        )))
    } else {
        Ok(None)
    }
}


#[tracing::instrument(skip_all)]
async fn delete_task(
    mut transaction: PgTransaction,
    issue_id: Uuid,
    email: &str,
) -> Result<(), anyhow::Error> {
    let query = sqlx::query!(
        r#"
        DELETE FROM issue_delivery_queue
        WHERE
            newsletter_issue_id = $1 AND
            subscriber_email = $2
        "#,
        issue_id,
        email,
    );
    transaction.execute(query).await?;
    transaction.commit().await?;
    Ok(())
}

struct NewsletterIssue {
    title: String,
    content: String,
}

#[tracing::instrument(skip_all)]
async fn get_issue(pool: &PgPool, issue_id: Uuid) -> Result<NewsletterIssue, anyhow::Error> {
    let issue = sqlx::query_as!(
        NewsletterIssue,
        r#"
        SELECT title, content
        FROM newsletter_issues
        WHERE
            newsletter_issue_id = $1
        "#,
        issue_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(issue)
}


#[tracing::instrument(skip_all)]
async fn store_for_retry(
    mut transaction: PgTransaction,
    issue_id: Uuid,
    email: &str,
    retries: i16,
) -> Result<(), anyhow::Error> {
    let backoff = ChronoDuration::minutes(2_i64.pow(retries as u32));
    let execute_after = Utc::now() + backoff;

    sqlx::query!(
        r#"
        UPDATE issue_delivery_queue
        SET
            n_retries = n_retries + 1,
            execute_after = $3
        WHERE
            newsletter_issue_id = $1 AND
            subscriber_email = $2
        "#,
        issue_id,
        email,
        execute_after,
    )
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;

    Ok(())
}


enum ExecutionOutcome {
    TaskCompleted,
    EmptyQueue,
}

async fn worker_loop(pool: PgPool, email_client: EmailClient) -> Result<(), anyhow::Error> {
    loop {
        match try_execute_task(&pool, &email_client).await {
            Ok(ExecutionOutcome::EmptyQueue) => {
                tokio::time::sleep(Duration::from_secs(10)).await;
            },
            Err(_) => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            },
            Ok(ExecutionOutcome::TaskCompleted) => {},
        }
    }
}

pub async fn run_worker_until_stopped(configuration: Settings) -> Result<(), anyhow::Error> {
    let connection_pool = get_connection_pool(&configuration.database);

    let (sender_name, sender_email) = configuration
        .email_client
        .sender_name_end_email()
        .expect("Invalid sender email address");

    let timeout = configuration.email_client.timeout();
    let email_client = EmailClient::new(
        configuration.email_client.base_url,
        sender_email,
        sender_name,
        configuration.email_client.authorization_token,
        timeout,
    );
    worker_loop(connection_pool, email_client).await
}
