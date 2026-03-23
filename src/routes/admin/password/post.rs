use actix_web::{HttpResponse, web};

use crate::{
    session_state::TypedSession,
    utils::{e500, see_other},
};

#[derive(serde::Deserialize)]
pub struct FormData {
    current_password: String,
    new_password: String,
    new_password_check: String,
}

pub async fn change_password(
    form: web::Form<FormData>,
    session: TypedSession,
) -> Result<HttpResponse, actix_web::Error> {
    if session.get_user_id().map_err(e500)?.is_none() {
        return Ok(see_other("/login"));
    }
    if form.new_password != form.new_password_check {
        return Ok(HttpResponse::BadRequest().body("New password and confirmation do not match"));
    }

    // Here you would typically verify the current password and update it in the database.
    // For demonstration purposes, we'll just return a success message.

    Ok(HttpResponse::Ok().body("Password changed successfully"))
}
