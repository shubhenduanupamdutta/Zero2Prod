use crate::helpers::spawn_app;
use rand::{distr::Alphanumeric, rng, Rng};
use uuid::Uuid;

#[tokio::test]
async fn expired_token_returns_unauthorized() {
    let app = spawn_app().await;

    // Insert a subscriber and a token in the database
    let id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, status)
        VALUES ($1, $2, $3, 'pending_confirmation')
        "#,
        id,
        "ursula_le_guin@example.com",
        "le guin"
    )
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert subscriber.");

    let mut rng = rng();
    let token: String = std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect();
    sqlx::query!(
        r#"
        INSERT INTO subscription_tokens (subscription_token, subscriber_id, created_at)
        VALUES ($1, $2, now() - INTERVAL '25 hours')
        "#,
        token,
        id
    )
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert subscription token.");

    // Act
    let response = reqwest::get(&format!(
        "{}/subscriptions/confirm?token={}",
        &app.address, token
    ))
    .await
    .expect("Failed to execute request.");

    // Assert
    assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
}
