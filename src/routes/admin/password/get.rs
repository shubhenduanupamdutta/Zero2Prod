use actix_web::{HttpResponse, http::header::ContentType};

pub async fn change_password_form() -> Result<HttpResponse, actix_web::Error> {
    Ok(HttpResponse::Ok().content_type(ContentType::html()).body(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=utf-8">
    <title>Change Password</title>
</head>
<body>
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
    ))
}
