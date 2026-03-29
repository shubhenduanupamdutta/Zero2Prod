use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use sqlx::PgPool;

use crate::{configuration::Settings, startup::get_connection_pool, utils::ExecutionOutcome};


async fn try_deleting_expired_idempotency_keys(
    pool: &PgPool,
) -> Result<ExecutionOutcome, anyhow::Error> {
    let current_time = Utc::now();
    let yesterday = current_time - Duration::days(1);
    let row_affected = sqlx::query!(
        r#"
        DELETE FROM idempotency
        WHERE created_at < $1
        "#,
        yesterday
    )
    .execute(pool)
    .await?
    .rows_affected();
    if row_affected > 0 {
        tracing::info!(
            deleted_rows = row_affected,
            "Idempotency key cleanup: deleted {} expired idempotency key(s).",
            row_affected
        );
        Ok(ExecutionOutcome::TaskCompleted)
    } else {
        tracing::debug!("Idempotency key cleanup: no expired idempotency keys to delete.");
        Ok(ExecutionOutcome::EmptyQueue)
    }
}


pub async fn idempotency_cleanup_worker(configuration: Settings) -> Result<(), anyhow::Error> {
    let pool = get_connection_pool(&configuration.database);

    loop {
        match try_deleting_expired_idempotency_keys(&pool).await {
            Ok(ExecutionOutcome::EmptyQueue) => {
                tokio::time::sleep(StdDuration::from_mins(60)).await;
            },
            Err(e) => {
                {
                    tracing::error!(
                        error = ?e,
                        "Idempotency key cleanup: failed to delete expired idempotency keys."
                    );
                }
                tokio::time::sleep(StdDuration::from_secs(1)).await;
            },
            Ok(ExecutionOutcome::TaskCompleted) => {},
        }
    }
}
