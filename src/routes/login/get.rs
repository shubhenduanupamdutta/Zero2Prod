use actix_web::{HttpResponse, http::header::ContentType};


pub async fn login_form() -> HttpResponse {
    let error_html = todo!();

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
    {error_html}
    <form action="/login" method="post">
        <label>Username
            <input type="text" name="username" placeholder="Enter username" required>
        </label>
        <label>Password
            <input type="password" name="password" placeholder="Enter password" required>
        </label>
        <button type="submit">Login</button>
    </form>
</body>
</html>"#
        ))
}
