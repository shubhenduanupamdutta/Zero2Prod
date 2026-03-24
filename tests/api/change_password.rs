use serde_json::json;
use uuid::Uuid;

use crate::helpers::{assert_is_redirect_to, spawn_app};

#[tokio::test]
async fn you_must_be_logged_in_to_see_the_change_password_form() {
    // Arrange
    let app = spawn_app().await;

    // Act
    let response = app.get_change_password().await;

    // Assert
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn you_must_be_logged_in_to_change_your_password() {
    // Arrange
    let app = spawn_app().await;
    let new_password = uuid::Uuid::new_v4().to_string();

    // Act
    let body = json!({
        "current_password": Uuid::new_v4().to_string(),
        "new_password": &new_password,
        "new_password_check": &new_password,
    });
    let response = app.post_change_password(&body).await;

    // Assert
    assert_is_redirect_to(&response, "/login");
}

#[tokio::test]
async fn new_password_fields_must_match() {
    // Arrange
    let app = spawn_app().await;
    let new_password = Uuid::new_v4().to_string();
    let another_new_password = Uuid::new_v4().to_string();

    // Act - Part 1 - Login
    app.post_login(&json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    // Act - Part 2 - Attempt to change password
    let response = app
        .post_change_password(&json!({
            "current_password": &app.test_user.password,
            "new_password": &new_password,
            "new_password_check": &another_new_password,
        }))
        .await;
    // Assert - Part 2 - Check that we are back to the change password form
    assert_is_redirect_to(&response, "/admin/password");

    // Act - Part 3 - Follow the redirect
    let html_page = app.get_change_password_html().await;
    assert!(html_page.contains(
        "<p><i>You entered two different new passwords - the field values must match.</i></p>"
    ))
}

#[tokio::test]
async fn current_password_must_be_valid() {
    // Arrange
    let app = spawn_app().await;
    let new_password = Uuid::new_v4().to_string();
    let wrong_password = Uuid::new_v4().to_string();

    // Act - Part 1 - Login
    app.post_login(&json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    // Act - Part 2 - Attempt to change password with an invalid current password
    let response = app
        .post_change_password(&json!({
            "current_password": &wrong_password,
            "new_password": &new_password,
            "new_password_check": &new_password,
        }))
        .await;
    // Assert - Part 2 - Check that we are back to the change password form
    assert_is_redirect_to(&response, "/admin/password");

    // Act - Part 3 - Follow the redirect
    let html_page = app.get_change_password_html().await;
    // Assert - Part 3 - Check that the error message is displayed
    assert!(html_page.contains("<p><i>The current password is incorrect.</i></p>"))
}


#[tokio::test]
async fn new_password_should_be_larger_than_12_characters() {
    // Arrange
    let app = spawn_app().await;
    let new_password = "a".repeat(11);

    // Act - Part 1 - Login
    app.post_login(&json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    // Act - Part 2 - Attempt to change password with an invalid new password
    let response = app
        .post_change_password(&json!({
            "current_password": &app.test_user.password,
            "new_password": &new_password,
            "new_password_check": &new_password,
        }))
        .await;
    // Assert - Part 2 - Check that we are back to the change password form
    assert_is_redirect_to(&response, "/admin/password");

    // Act - Part 3 - Follow the redirect
    let html_page = app.get_change_password_html().await;
    // Assert - Part 3 - Check that the error message is displayed
    assert!(
        html_page
            .contains("<p><i>The new password must be between 12 and 128 characters long.</i></p>")
    )
}

#[tokio::test]
async fn new_password_should_be_smaller_than_128_characters() {
    // Arrange
    let app = spawn_app().await;
    let new_password = "a".repeat(129);

    // Act - Part 1 - Login
    app.post_login(&json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    }))
    .await;

    // Act - Part 2 - Attempt to change password with an invalid new password
    let response = app
        .post_change_password(&json!({
            "current_password": &app.test_user.password,
            "new_password": &new_password,
            "new_password_check": &new_password,
        }))
        .await;
    // Assert - Part 2 - Check that we are back to the change password form
    assert_is_redirect_to(&response, "/admin/password");

    // Act - Part 3 - Follow the redirect
    let html_page = app.get_change_password_html().await;
    // Assert - Part 3 - Check that the error message is displayed
    assert!(
        html_page
            .contains("<p><i>The new password must be between 12 and 128 characters long.</i></p>")
    )
}


#[tokio::test]
async fn changing_password_works() {
    // Arrange
    let app = spawn_app().await;
    let new_password = Uuid::new_v4().to_string();

    // Act - Part 1 - Login
    let login_body = json!({
        "username": &app.test_user.username,
        "password": &app.test_user.password,
    });
    let response = app.post_login(&login_body).await;
    // Assert - Part 1 - Check that we are logged in
    assert_is_redirect_to(&response, "/admin/dashboard");

    // Act - Part 2 - Change password
    let response = app
        .post_change_password(&json!({
            "current_password": &app.test_user.password,
            "new_password": &new_password,
            "new_password_check": &new_password,
        }))
        .await;
    // Assert - Part 2 - Check that we are back to password change form with a success message
    assert_is_redirect_to(&response, "/admin/password");

    // Act - Part 3 - Follow the redirect
    let html_page = app.get_change_password_html().await;
    // Assert - Part 3 - Check that the success message is displayed
    assert!(html_page.contains("<p><i>Your password has been changed.</i></p>"));

    // Act - Part 4 - Logout
    let response = app.post_logout().await;
    assert_is_redirect_to(&response, "/login");

    // Act - Part 5 - Follow the redirect
    let html_page = app.get_login_html().await;
    // Assert - Part 5 - Check that the login page is displayed
    assert!(html_page.contains("<p><i>You have successfully logged out.</i></p>"));

    // Act - Part 6 - Login with the new password
    let login_body = json!({
        "username": &app.test_user.username,
        "password": &new_password,
    });
    let response = app.post_login(&login_body).await;
    // Assert - Part 6 - Check that we are logged in with the new password
    assert_is_redirect_to(&response, "/admin/dashboard");
}
