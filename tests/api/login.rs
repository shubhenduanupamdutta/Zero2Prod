use serde_json::json;

use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let login_body = json!({
        "username": "random-username",
        "password": "random-password",
    });
    let response = app.post_login(&login_body).await;

    // Assert
    response
        .cookies()
        .find(|cookie| cookie.name() == "_flash")
        .expect("No _flash cookie was found in the response");
    assert_is_redirect_to(&response, "/login");

    // Act - Part 2
    let html_page = app.get_login_html().await;
    // Assert - Part 2
    // This time there should be a flash message in the HTML page
    assert!(html_page.contains(r#"<p><i>Authentication failed</i></p>"#));

    // Act - Part 3
    let html_page = app.get_login_html().await;
    // Assert - Part 3
    // The flash message should be consumed and not appear in the HTML page
    assert!(!html_page.contains("Authentication failed"));
}

#[tokio::test]
async fn redirect_to_admin_dashboard_after_login_success() {
    // Arrange
    let app = spawn_app().await;

    // Act - Part 1 - Login
    let login_body = json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    });
    let response = app.post_login(&login_body).await;

    // Assert - Part 1
    assert_is_redirect_to(&response, "/admin/dashboard");

    // Act - Part 2 - Follow the redirect
    let html_page = app.get_admin_dashboard_html().await;
    assert!(html_page.contains(&format!("Welcome {}!", &app.test_user.username)));
}
