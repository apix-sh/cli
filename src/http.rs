use crate::error::ApixError;
use std::io::Read;

/// Reads a ureq response body into a String with a large safety limit (500MB).
/// This avoids ureq's default 10MB limit which is too small for large OpenAPI specs.
pub fn read_response(resp: ureq::Response) -> Result<String, ApixError> {
    let mut content = String::new();
    resp.into_reader()
        .take(500 * 1024 * 1024) // 500 MB safety limit
        .read_to_string(&mut content)
        .map_err(|err| ApixError::Http(format!("Failed to read response body: {err}")))?;
    Ok(content)
}
