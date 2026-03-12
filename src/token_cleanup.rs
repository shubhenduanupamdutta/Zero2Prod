use std::time::Duration as StdDuration;

use chrono::Utc;
use sqlx::PgPool;

/// Spawns a background tokio task that periodically deletes expired tokens.
/// The task runs until the process exits and cannot be cancelled externally.
///
/// # Arguments
/// * `pool`             – Connection pool shared with the HTTP server.
/// * `interval_hours`   – How frequently to run the cleanup (hours).
/// * `retention_hours`  – Delete tokens older than this many hours.
pub fn spawn_token_cleanup_task(pool: PgPool, interval_hours: u32, retention_hours: u32) {
    tokio::spawn(async move {
        let interval = StdDuration::from_hours(interval_hours as u64);
        let mut ticker = tokio::time::interval(interval);

        loop {
            ticker.tick().await; // first tick fires immediately at t=0
            match delete_expired_tokens(&pool, retention_hours).await {
                Ok(rows) => {
                    if rows > 0 {
                        tracing::info!(
                            deleted_rows = rows,
                            "Token cleanup: deleted {} expired subscription token(s).",
                            rows
                        );
                    } else {
                        tracing::debug!("Token cleanup: no expired tokens to delete.");
                    }
                },
                Err(e) => {
                    tracing::error!(
                        error = ?e,
                        "Token cleanup: failed to delete expired tokens."
                    );
                    // Do NOT panic — a transient DB error should not crash the server.
                    // The next tick will retry.
                },
            }
        }
    });
}

#[tracing::instrument(name = "Delete expired subscription tokens", skip(pool))]
async fn delete_expired_tokens(pool: &PgPool, retention_hours: u32) -> Result<u64, sqlx::Error> {
    let older_than = Utc::now() - StdDuration::from_hours(retention_hours as u64);
    let result = sqlx::query!(
        r#"
        DELETE FROM subscription_tokens
        WHERE created_at < $1
        "#,
        older_than
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
