use secrecy::{ExposeSecret as _, SecretString};

pub fn password_is_of_valid_length(password: &SecretString) -> bool {
    if password.expose_secret().is_empty() {
        return false;
    };

    let password_length = password.expose_secret().chars().count();
    (12..=128).contains(&password_length)
}
