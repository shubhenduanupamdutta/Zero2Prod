use sqlx::{Executor, PgPool, Postgres, Transaction};
use tracing::{Span, field::display};
use uuid::Uuid;

use crate::{domain::NewSubscriber, email_client::EmailClient};

#[tracing::instrument(
    skip_all,
    fields(
        newsletter_issue_id=tracing::field::Empty,
        subscriber_email=tracing::field::Empty,
    ),
    err
)]
async fn try_execute_task(pool: &PgPool, email_client: &EmailClient) -> Result<(), anyhow::Error> {
    if let Some((transaction, issue_id, email, name)) = dequeue_task(pool).await? {
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
                    tracing::error!(
                        error.cause_chain = ?e,
                        error.message = %e,
                        "Failed to send newsletter issue email to subscriber. Skipping."
                    );
                }
            },
            Err(e) => {
                tracing::error!(
                    error.cause_chain = ?e,
                    error.message = %e,
                    "Skipping a confirmed subscriber. Their stored contact details are invalid"
                );
            },
        }

        delete_task(transaction, issue_id, &email).await?;
    }

    Ok(())
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
/// * `Ok(Some((transaction, issue_id, email, name)))` - If a task is available and has been locked
///   for processing
/// * `Ok(None)` - If no task is available for processing
async fn dequeue_task(
    pool: &PgPool,
) -> Result<Option<(PgTransaction, Uuid, String, String)>, anyhow::Error> {
    // We need to start a transaction to be able to lock the selected task
    let mut transaction = pool.begin().await?;
    let record = sqlx::query!(
        r#"
        SELECT q.newsletter_issue_id, q.subscriber_email, s.name AS subscriber_name
        FROM issue_delivery_queue q
        JOIN subscriptions s ON s.email = q.subscriber_email
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
