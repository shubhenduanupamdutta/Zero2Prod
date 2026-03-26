use std::fmt::Write;

use actix_web::{HttpResponse, http::header::ContentType, web};
use actix_web_flash_messages::IncomingFlashMessages;

use crate::authentication::UserId;


#[tracing::instrument(name = "Submit newsletter form", skip(flash_messages))]
pub async fn submit_newsletter_form(
    _user_id: web::ReqData<UserId>,
    flash_messages: IncomingFlashMessages,
) -> HttpResponse {
    let mut message_html = String::new();
    for m in flash_messages.iter() {
        writeln!(message_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=UTF-8">
    <title>Login</title>
</head>
<body>
    {message_html}
    <form action="/admin/newsletters" method="post">
        <label>Title
            <input type="text" name="title" placeholder="Enter newsletter title here" required>
        </label>
        <br />
        <label>Content
            <textarea name="content" placeholder="Enter newsletter content here" required></textarea>
        </label>
        <button type="submit">Login</button>
    </form>
</body>
</html>"#
        ))
}
