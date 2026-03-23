use std::{error, fmt};

/// # Format an error chain for human consumption
///
/// This is not intended to be used for debugging, but rather to be user-friendly. The error chain
/// is formatted in a way that is easy to read and understand, with each error in the chain
/// separated by a newline and indented for clarity.
///
/// # Args
/// * `e` - The error to format. This can be any type that implements the `std::error::Error` trait.
/// * `f` - The formatter to write the formatted error chain to. This is typically provided by the
///   `fmt::Display` implementation of the error type.
///
/// # Returns
/// A `fmt::Result` indicating whether the formatting was successful. If an error occurs during
/// formatting, the error will be returned as a `fmt::Error`.
pub fn error_chain_fmt(e: &impl error::Error, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "{}", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "\nCaused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}

/// Return an opaque 500 while preserving the error's root cause for logging
///
/// This function takes an error of any type that implements the `fmt::Debug` and `fmt::Display`
/// traits, and returns an `actix_web::Error` that represents an internal server error (HTTP 500).
/// The original error is preserved for logging purposes, but the client will receive a generic
/// error message without any sensitive information.
///
/// # Args
/// * `e` - The error to convert into an internal server error. This can be any type that implements
///   the `fmt::Debug` and `fmt::Display` traits, and must have a static lifetime.
///
/// # Returns
/// An `actix_web::Error` that represents an internal server error (HTTP 500). The original error is
///  preserved for logging, but the client will receive a generic error message.
pub fn e500<T>(e: T) -> actix_web::Error
where
    T: fmt::Debug + fmt::Display + 'static,
{
    actix_web::error::ErrorInternalServerError(e)
}
