use crate::helpers::{generate_token, spawn_app, ConfirmationLinks};
use chrono::{Duration, Utc};
use reqwest::StatusCode;
use serde_json::Value;
use wiremock::{
    matchers::{method, path},
    Mock, ResponseTemplate,
};

#[tokio::test]
async fn confirmations_without_token_are_rejected_with_a_400() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = reqwest::get(&format!("{}/subscriptions/confirm", app.address))
        .await
        .expect("Failed to execute request.");

    // Assert
    assert_eq!(400, response.status().as_u16());
}

#[tokio::test]
async fn the_link_returned_by_subscribe_returns_a_200_if_called() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    let email_request = &app.email_server.received_requests().await.unwrap()[0];

    let ConfirmationLinks {
        link,
    } = app.get_confirmation_links(email_request);

    // Act
    let response = reqwest::get(link)
        .await
        .expect("Failed to execute request.");

    // Assert
    assert_eq!(200, response.status().as_u16());
}

#[tokio::test]
async fn clicking_on_the_confirmation_link_confirms_a_subscriber() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    app.post_subscriptions(body.into()).await;

    let email_request = &app.email_server.received_requests().await.unwrap()[0];

    let ConfirmationLinks {
        link,
    } = app.get_confirmation_links(email_request);

    // Act
    reqwest::get(link)
        .await
        .unwrap()
        .error_for_status()
        .unwrap();

    // Assert
    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions",)
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch saved subscription.");
    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "confirmed");
}

/// Test case when the confirmation link is clicked twice. The first click should confirm the
/// subscriber, and the second click should return a 200 OK status, indicating that the subscriber
/// is already confirmed.
#[tokio::test]
async fn confirm_twice_returns_already_confirmed() {
    // Arrange
    let app = spawn_app().await;
    let (name, email) = ("le guin", "ursula_le_guin%40gmail.com");

    let id = app.insert_subscriber(email, name, Some("confirmed")).await;
    let now = Utc::now();
    let token = generate_token();
    app.insert_subscription_token(id, &token, now, Some(now))
        .await;

    // Act
    let response = app.confirm_subscriptions(&token).await;

    // Assert
    assert_eq!(response.status().as_u16(), 200);
    let response: Value = response.json().await.unwrap();
    assert_eq!(response["status"], "already_confirmed");
}

/// Test case that confirm marks the token as consumed. When confirm endpoint is hit, the
/// status is changed to confirmed and token is marked as consumed.
#[tokio::test]
async fn confirm_marks_token_as_consumed() {
    // Arrange
    let app = spawn_app().await;
    let (name, email) = ("le guin", "ursula_le_guin%40gmail.com");
    let id = app.insert_subscriber(email, name, None).await;
    let token = generate_token();
    app.insert_subscription_token(id, &token, Utc::now(), None)
        .await;

    // Act
    app.confirm_subscriptions(&token).await;

    // Assert
    let record = sqlx::query!(
        r#"SELECT consumed_at FROM subscription_tokens WHERE subscription_token = $1"#,
        token
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch subscription token.");
    assert!(record.consumed_at.is_some());

    let record = sqlx::query!(r#"SELECT status FROM subscriptions WHERE id = $1"#, id)
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch subscription.");
    assert_eq!(record.status, "confirmed");
}

/// Test case where when a confirmation link is clicked second time, there is no change in the
/// database and the response indicates that the subscriber is already confirmed.
#[tokio::test]
async fn confirm_twice_does_not_change_database() {
    // Arrange
    let app = spawn_app().await;
    let (name, email) = ("le guin", "ursula_le_guin%40gmail.com");
    let id = app.insert_subscriber(email, name, Some("confirmed")).await;
    let token = generate_token();
    let now = Utc::now();
    app.insert_subscription_token(id, &token, now, Some(now))
        .await;

    // Act
    app.confirm_subscriptions(&token).await;

    // Assert
    let record = sqlx::query!(
        r#"SELECT consumed_at FROM subscription_tokens WHERE subscription_token = $1"#,
        token
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch subscription token.");
    assert_eq!(record.consumed_at.unwrap(), now);
}

/// Test case where when a second different confirmation is clicked where status is already
/// confirmed due to previous token being consumed successfully, there is no change in the
/// database and the response indicates that the subscriber is already confirmed. And new token
/// is marked as consumed.
#[tokio::test]
async fn confirm_with_second_token_after_first_consumed() {
    // Arrange
    let app = spawn_app().await;
    let (name, email) = ("le guin", "ursula_le_guin%40gmail.com");
    let id = app.insert_subscriber(email, name, Some("confirmed")).await;
    let token1 = generate_token();
    let token2 = generate_token();
    let now = Utc::now();
    app.insert_subscription_token(id, &token1, now, Some(now))
        .await;
    app.insert_subscription_token(id, &token2, now, None).await;

    // Act
    let response = app.confirm_subscriptions(&token2).await;

    // Assert
    let record = sqlx::query!(
        r#"SELECT consumed_at FROM subscription_tokens WHERE subscription_token = $1"#,
        token2
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch subscription token.");
    assert!(record.consumed_at.is_some());

    let response = response.json::<Value>().await.unwrap();
    assert_eq!(response["status"], "already_confirmed");
}

/// Test case where user is already confirmed with a token, but the token has now expired. Upon
/// clicking the confirmation link again, user should get already confirmed response rather than
/// unauthorized response.
#[tokio::test]
async fn consumed_token_past_expiry_returns_already_confirmed() {
    // Arrange
    let app = spawn_app().await;
    let (name, email) = ("le guin", "ursula_le_guin%40gmail.com");
    let id = app.insert_subscriber(email, name, Some("confirmed")).await;
    let token = generate_token();
    let created_at = Utc::now() - Duration::hours(25);
    app.insert_subscription_token(id, &token, created_at, Some(created_at))
        .await;

    // Act
    let response = app.confirm_subscriptions(&token).await;

    // Assert
    assert_eq!(response.status().as_u16(), StatusCode::OK);
    let response = response.json::<Value>().await.unwrap();
    assert_eq!(response["status"], "already_confirmed");
}

