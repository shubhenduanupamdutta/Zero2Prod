use serde_json::json;
use wiremock::{
    Mock,
    ResponseTemplate,
    matchers::{any, method, path},
};

use crate::helpers::{ConfirmationLinks, TestApp, assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn newsletters_are_not_delivered_to_unconfirmed_subscribers() {
    // Arrange
    let app = spawn_app().await;

    create_unconfirmed_subscriber(&app).await;
    app.test_user.login(&app).await;

    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .named("No newsletter delivery to unconfirmed subscribers")
        .expect(0)
        .mount(&app.email_server)
        .await;

    // Act - Part 1 - Submit Newsletter form
    let newsletter_request_body = json!({
        "title": "Newsletter title",
        "content": "<p>Newsletter body as HTML</p>"
    });

    let response = app.post_publish_newsletter(&newsletter_request_body).await;
    // Assert - Part 1 - Check that we are redirected back to the newsletter form
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 2 - Follow th redirect
    let html_page = app.get_publish_newsletter_html().await;
    // Assert - Part 2 - Newsletter has been published
    assert!(html_page.contains("<p><i>The newsletter issue has been published!</i></p>"));
}

#[tokio::test]
async fn newsletters_are_delivered_to_confirmed_subscribers() {
    // Arrange
    let app = spawn_app().await;
    create_confirmed_subscriber(&app).await;
    app.test_user.login(&app).await;

    Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .named("Newsletter delivery to confirmed subscribers")
        .expect(1)
        .mount(&app.email_server)
        .await;

    // Act - Part 1 - Submit Newsletter form
    let newsletter_request_body = serde_json::json!({
        "title": "Newsletter title",
        "content": "<p>Newsletter body as HTML</p>"
    });
    let response = app.post_publish_newsletter(&newsletter_request_body).await;
    // Assert - Part 1 - Check that we are redirected back to the newsletter form
    assert_is_redirect_to(&response, "/admin/newsletters");

    // Act - Part 2 - Follow th redirect
    let html_page = app.get_publish_newsletter_html().await;

    // Assert - Part 2 - Newsletter has been published
    assert!(html_page.contains("<p><i>The newsletter issue has been published!</i></p>"));
}

#[tokio::test]
async fn you_must_be_logged_in_to_see_the_newsletter_form() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get_publish_newsletter().await;

    // Assert
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn you_must_be_logged_in_to_publish_a_newsletter() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let newsletter_request_body = json!({
        "title": "Newsletter title",
        "content": "<p>Newsletter body as HTML</p>"
    });
    let response = app.post_publish_newsletter(&newsletter_request_body).await;

    // Assert
    assert_is_redirect_to(&response, "/login");
}

/// Use the public API of the application under test to create an unconfirmed subscriber.
async fn create_unconfirmed_subscriber(app: &TestApp) -> ConfirmationLinks {
    let body = "name=le%20guin&email=ursula_le_guin%40example.com";

    let _mock_guard = Mock::given(path("/email"))
        .and(method("POST"))
        .respond_with(ResponseTemplate::new(200))
        .named("Create unconfirmed subscriber")
        .expect(1)
        .mount_as_scoped(&app.email_server)
        .await;

    app.post_subscriptions(body.into())
        .await
        .error_for_status()
        .unwrap();

    // We now inspect the request received by the mock email server to retrieve
    // the confirmation link and return it
    let email_request = &app
        .email_server
        .received_requests()
        .await
        .unwrap()
        .pop()
        .unwrap();
    app.get_confirmation_links(email_request)
}

async fn create_confirmed_subscriber(app: &TestApp) {
    // We can reuse the `create_unconfirmed_subscriber` function to create a confirmed subscriber
    // by simply following the confirmation link it returns.

    let confirmation_link = create_unconfirmed_subscriber(app).await;

    reqwest::get(confirmation_link.link)
        .await
        .expect("Failed to execute request")
        .error_for_status()
        .unwrap();
}
