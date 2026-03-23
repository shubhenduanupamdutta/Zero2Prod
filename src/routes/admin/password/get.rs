use std::fmt::Write;

use actix_web::{HttpResponse, http::header::ContentType};
use actix_web_flash_messages::IncomingFlashMessages;

use crate::{
    session_state::TypedSession,
    utils::{e500, see_other},
};

pub async fn change_password_form(
    session: TypedSession,
    flash_messages: IncomingFlashMessages,
) -> Result<HttpResponse, actix_web::Error> {
    if session.get_user_id().map_err(e500)?.is_none() {
        return Ok(see_other("/login"));
    }
    let mut message_html = String::new();
    for m in flash_messages.iter() {
        writeln!(message_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }
    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=utf-8">
    <title>Change Password</title>
</head>
<body>
    {message_html}
    <form action="/admin/password" method="post">
        <label>Current Password
            <input type="password" name="current_password" placeholder="Current Password">
        </label>
        <br>
        <label>New Password
            <input type="password" name="new_password" placeholder="New Password">
        </label>
        <br>
        <label>Confirm New Password
            <input type="password" name="new_password_check" placeholder="Confirm New Password">
        </label>
        <br>
        <button type="submit">Change Password</button>
    </form>
    <p><a href="/admin/dashboard">&lt; - Back</a></p>
</body>
</html>"#,
        )))
}
