use crate::helpers::spawn_app;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn subscribe_returns_a_200_for_valid_form_data() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    // Act

    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert_eq!(200, response.status().as_u16());
}

#[tokio::test]
async fn subscribe_persists_the_new_subscriber() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    // Act
    let response = app.post_subscriptions(body.into()).await;

    // Assert
    assert!(response.status().is_success());

    let saved = sqlx::query!("SELECT email, name, status FROM subscriptions",)
        .fetch_one(&app.db_pool)
        .await
        .expect("Failed to fetch saved subscription.");

    assert_eq!(saved.email, "ursula_le_guin@gmail.com");
    assert_eq!(saved.name, "le guin");
    assert_eq!(saved.status, "pending_confirmation");
}

#[tokio::test]
async fn subscribe_returns_a_400_when_data_is_missing() {
    // Arrange
    let app = spawn_app().await;

    let test_cases = vec![
        ("name=le%20guin", "missing the email"),
        ("email=ursula_le_guin%40gmail.com", "missing the name"),
        ("", "missing both name and email"),
    ];

    for (invalid_body, error_message) in test_cases {
        // Act
        let response = app.post_subscriptions(invalid_body.into()).await;

        // Assert
        assert_eq!(
            response.status().as_u16(),
            400,
            "The API did not fail with 400 Bad Request when the payload was {}",
            error_message
        );
    }
}

#[tokio::test]
async fn subscribe_returns_a_400_when_fields_are_present_but_invalid() {
    // Arrange
    let app = spawn_app().await;
    let test_cases = vec![
        ("name=&email=ursula_le_guin%40gmail.com", "empty name"),
        ("name=Ursula&email=", "empty email"),
        ("name=Ursula&email=definitely_not_an_email", "invalid email"),
    ];

    for (body, description) in test_cases {
        // Act
        let response = app.post_subscriptions(body.into()).await;

        // Assert
        assert_eq!(
            response.status().as_u16(),
            400,
            "The API did not return 400 Bad Request when the payload was {}",
            description
        );
    }
}

#[tokio::test]
async fn subscribe_sends_a_confirmation_email_for_valid_data() {
    // Arrange
    let app = spawn_app().await;

    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Act
    app.post_subscriptions(body.into()).await;

    // Assert
    // Mock asserts on drop
}

#[tokio::test]
async fn subscribe_sends_a_confirmation_email_with_a_link() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.email_server)
        .await;

    // Act
    app.post_subscriptions(body.into()).await;

    // Assert
    // Get the first intercepted request
    let email_request = &app.email_server.received_requests().await.unwrap()[0];

    let confirmation_links = app.get_confirmation_links(email_request);
    assert!(
        confirmation_links.link.as_str().starts_with("http://")
            || confirmation_links.link.as_str().starts_with("https://")
    );
}

#[tokio::test]
async fn subscribe_twice_pending_sends_two_confirmation_emails() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&app.email_server)
        .await;

    // Act
    app.post_subscriptions(body.into()).await;
    app.post_subscriptions(body.into()).await;

    // Assert
    // Mock asserts on drop
}

#[tokio::test]
async fn subscribe_twice_pending_generates_different_confirmation_tokens() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&app.email_server)
        .await;

    // Act
    app.post_subscriptions(body.into()).await;
    app.post_subscriptions(body.into()).await;

    // Assert
    let email_requests = app.email_server.received_requests().await.unwrap();
    let confirmation_links_1 = app.get_confirmation_links(&email_requests[0]);
    let confirmation_links_2 = app.get_confirmation_links(&email_requests[1]);

    assert_ne!(
        confirmation_links_1.link, confirmation_links_2.link,
        "The confirmation links were the same for two pending subscriptions"
    );
}

#[tokio::test]
async fn subscribe_after_confirm_returns_a_200_and_sends_a_email_with_no_link() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Insert a confirmed subscriber into the database
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, status, subscribed_at)
        VALUES ($1, $2, $3, 'confirmed', $4)
        "#,
        uuid::Uuid::new_v4(),
        "ursula_le_guin@gmail.com",
        "le guin",
        chrono::Utc::now()
    )
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert confirmed subscriber.");

    // Act
    app.post_subscriptions(body.into()).await;

    // Assert
    // Mock asserts on drop
    // Checking the email content for the absence of a confirmation link
    let email_request = &app.email_server.received_requests().await.unwrap()[0];
    let body: serde_json::Value = serde_json::from_slice(&email_request.body).unwrap();

    // Extract the link from one of the request fields
    let links = linkify::LinkFinder::new()
        .links(body["htmlbody"].as_str().unwrap())
        .filter(|l| *l.kind() == linkify::LinkKind::Url)
        .collect::<Vec<_>>();
    assert!(
        links.is_empty(),
        "Expected no confirmation link for already confirmed subscriber"
    );
}

#[tokio::test]
async fn both_token_works_for_pending_subscriber() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&app.email_server)
        .await;

    // Act
    app.post_subscriptions(body.into()).await;
    app.post_subscriptions(body.into()).await;

    // Assert
    let email_requests = app.email_server.received_requests().await.unwrap();
    let confirmation_links_1 = app.get_confirmation_links(&email_requests[0]);
    let confirmation_links_2 = app.get_confirmation_links(&email_requests[1]);

    assert_ne!(
        confirmation_links_1.link, confirmation_links_2.link,
        "The confirmation links were the same for two pending subscriptions"
    );
    // Both links should work

    // First link should work
    let response_1 = reqwest::get(confirmation_links_1.link).await.unwrap();
    assert!(response_1.status().is_success());
    // Check in database that the subscriber is confirmed
    let saved = sqlx::query!(
        "SELECT status FROM subscriptions WHERE email = $1",
        "ursula_le_guin@gmail.com"
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch saved subscription.");
    assert_eq!(saved.status, "confirmed");

    // Reset confirmation status
    sqlx::query!(
        "UPDATE subscriptions SET status = 'pending_confirmation' WHERE email = $1",
        "ursula_le_guin@gmail.com"
    )
    .execute(&app.db_pool)
    .await
    .expect("Failed to reset confirmation status.");

    // Second link should work
    let response_2 = reqwest::get(confirmation_links_2.link).await.unwrap();
    assert!(response_2.status().is_success());
    // Check in database that the subscriber is confirmed
    let saved = sqlx::query!(
        "SELECT status FROM subscriptions WHERE email = $1",
        "ursula_le_guin@gmail.com"
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch saved subscription.");
    assert_eq!(saved.status, "confirmed");
}

#[tokio::test]
async fn second_subscriber_does_not_change_subscriber_id() {
    // Arrange
    let app = spawn_app().await;
    let body = "name=le%20guin&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&app.email_server)
        .await;

    // Act
    app.post_subscriptions(body.into()).await;
    let saved = sqlx::query!(
        "SELECT id FROM subscriptions WHERE email = $1",
        "ursula_le_guin@gmail.com"
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch saved subscription.");
    let first_id = saved.id;
    app.post_subscriptions(body.into()).await;

    // Assert

    let saved = sqlx::query!(
        "SELECT id FROM subscriptions WHERE email = $1",
        "ursula_le_guin@gmail.com"
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch saved subscription.");
    let second_id = saved.id;

    assert_eq!(
        first_id, second_id,
        "Subscriber ID changed after second subscription attempt"
    );
}

/// Test for the case when a subscriber tries to subscribe with the same email but a different name
/// while the subscription is still pending confirmation. The expected behavior is that the system
/// should not create a new subscription, don't change the name but send a new email with new token.
#[tokio::test]
async fn subscribe_with_different_name_same_email_pending() {
    // Arrange
    let app = spawn_app().await;
    let body_1 = "name=le%20guin&email=ursula_le_guin%40gmail.com";
    let body_2 = "name=ursula&email=ursula_le_guin%40gmail.com";

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&app.email_server)
        .await;

    // Act
    app.post_subscriptions(body_1.into()).await;
    app.post_subscriptions(body_2.into()).await;

    // Assert
    let saved = sqlx::query!(
        "SELECT name FROM subscriptions WHERE email = $1",
        "ursula_le_guin@gmail.com"
    )
    .fetch_one(&app.db_pool)
    .await
    .expect("Failed to fetch saved subscription.");
    assert_eq!(saved.name, "le guin");
}

/// This test ensures that if someone subscribes, who is already confirmed, with same email but
/// different name, nothing happens but a 200 OK is returned. This is to prevent a malicious user
/// from changing the name of an already confirmed subscriber.
#[tokio::test]
async fn subscribe_with_different_name_confirmed() {
    let app = spawn_app().await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Insert a subscriber and a token in the database
    let id = Uuid::new_v4();
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, now(), 'confirmed')
        "#,
        id,
        "ursula_le_guin@gmail.com",
        "le guin"
    )
    .execute(&app.db_pool)
    .await
    .expect("Failed to insert confirmed subscriber.");

    let body = "name=ursula&email=ursula_le_guin%40gmail.com";
    let response = app.post_subscriptions(body.into()).await;
    assert_eq!(response.status().as_u16(), 200);
}
