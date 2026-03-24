mod get;
mod post;

pub use get::change_password_form;
pub use post::{change_password, reject_anonymous_users};
