use crate::helpers::{generate_token, spawn_app};
use chrono::{Duration, Utc};

#[tokio::test]
async fn expired_token_returns_unauthorized() {
    let app = spawn_app().await;

    // Insert a subscriber and a token in the database

    let id = app
        .insert_subscriber("ursula_le_guin@example.com", "le guin", None)
        .await;
    let token = generate_token();
    app.insert_subscription_token(id, &token, Utc::now() - Duration::hours(25), None)
        .await;

    // Act
    let response = reqwest::get(&format!(
        "{}/subscriptions/confirm?subscription_token={}",
        &app.address, token
    ))
    .await
    .expect("Failed to execute request.");

    // Assert
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}
