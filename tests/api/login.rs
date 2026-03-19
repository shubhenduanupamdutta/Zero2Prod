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
    let flash_cookie = response
        .cookies()
        .find(|cookie| cookie.name() == "_flash")
        .expect("No _flash cookie was found in the response");
    assert_eq!(flash_cookie.value(), "Authentication failed");
    assert_is_redirect_to(&response, "/login");
}
