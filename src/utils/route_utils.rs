use actix_web::{HttpResponse, http::header::LOCATION};

/// Utility function to create a 303 See Other response with a Location header
///
/// # Arguments
/// * `location` - The URL to redirect the client to
///
/// # Returns
/// An `HttpResponse` with status 303 and a Location header set to the provided URL
pub fn see_other(location: &str) -> HttpResponse {
    HttpResponse::SeeOther()
        .insert_header((LOCATION, location))
        .finish()
}
