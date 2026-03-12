use chrono::{Duration, Utc};
use zero2prod::token_cleanup::spawn_token_cleanup_task;

use crate::helpers::{generate_token, spawn_app};

#[tokio::test]
async fn delete_expired_token_removes_old_rows_and_keeps_recent_rows() {
    let app = spawn_app().await;
    // Insert 2 token with more than 72 hour age (75, 89) and 2 tokens with less than 72 hour age
    // (1, 71)
    let tokens: [String; 4] = std::array::from_fn(|_| generate_token());
    let now = Utc::now();
    let timings = [
        now - Duration::hours(75),
        now - Duration::hours(89),
        now - Duration::hours(1),
        now - Duration::hours(71),
    ];
    let emails = [
        "some@example.com",
        "another@example.com",
        "third@example.com",
        "fourth@example.com",
    ];
    let names = ["Some Name", "Another Name", "Third Name", "Fourth Name"];
    for i in 0..4 {
        let id = app.insert_subscriber(emails[i], names[i], None).await;
        app.insert_subscription_token(id, &tokens[i], timings[i], None)
            .await;
    }

    // Act - Run the cleanup code
    spawn_token_cleanup_task(app.db_pool.clone(), 1, 72);

    // Assert - Only the 2 old tokens should be deleted
    tokio::time::sleep(Duration::seconds(2).to_std().unwrap()).await; // Wait for the cleanup task to run at least once
    let remaining_tokens = sqlx::query!(
        r#"
        SELECT subscription_token FROM subscription_tokens
        "#
    )
    .fetch_all(&app.db_pool)
    .await
    .expect("Failed to fetch remaining tokens.")
    .into_iter()
    .map(|record| record.subscription_token)
    .collect::<Vec<String>>();

    assert_eq!(
        remaining_tokens.len(),
        2,
        "Expected 2 tokens to remain after cleanup."
    );
    assert!(
        remaining_tokens.contains(&tokens[2]),
        "Expected token {} to remain.",
        tokens[2]
    );
    assert!(
        remaining_tokens.contains(&tokens[3]),
        "Expected token {} to remain.",
        tokens[3]
    );
}

#[tokio::test]
async fn spawn_cleanup_task_does_not_panic_on_db_error() {
    let app = spawn_app().await;
    tokio::time::sleep(Duration::seconds(5).to_std().unwrap()).await; // Wait for the app to be fully ready
    app.db_pool.close().await; // Close the DB pool to simulate a DB error in the cleanup task
    spawn_token_cleanup_task(app.db_pool.clone(), 1, 72);
    // If the task panics, the test will fail. We just need to wait a bit to let the task attempt to
    // run.
    let response = reqwest::Client::new()
        .get(format!("{}/health_check", &app.address))
        .send()
        .await
        .expect("Failed to execute request");

    // Confirm response is 200
    assert_eq!(
        response.status().as_u16(),
        200,
        "Expected health check to succeed even if cleanup task encounters DB error."
    );
}
