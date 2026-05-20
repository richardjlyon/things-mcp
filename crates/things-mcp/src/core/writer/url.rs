//! Compose `things:///json?data=…&auth-token=…` URLs from rendered operations.
//!
//! - All non-alphanumeric characters in `data` are percent-encoded (the strict
//!   set — Things' parser handles a wide-encoding without complaint, but the
//!   conservative form avoids any ambiguity).
//! - The `auth-token` segment is included iff a token is supplied.
//! - `mask_auth_token` exists so callers can log the URL without leaking
//!   the token.

use crate::core::writer::operation::Operation;
use crate::core::writer::secret::SecretString;

/// Build the full Things URL for one or more operations.
///
/// `auth_token` is `Some` when any operation in the batch requires it
/// (typically updates). For pure creates, pass `None`.
pub fn build_url(ops: &[Operation], auth_token: Option<&SecretString>) -> String {
    let payload: Vec<_> = ops.iter().flat_map(|op| op.render_batch()).collect();
    let minified =
        serde_json::to_string(&payload).expect("operations always serialise to valid JSON");
    let encoded_data = urlencoding::encode(&minified);
    let mut url = format!("things:///json?data={encoded_data}");
    if let Some(token) = auth_token {
        let encoded_token = urlencoding::encode(token.expose_secret()).into_owned();
        url.push_str("&auth-token=");
        url.push_str(&encoded_token);
    }
    url
}

/// Replace the `auth-token=…` segment of a URL with `auth-token=***` so the
/// URL is safe to log. If the URL has no auth-token segment, it's returned
/// unchanged.
pub fn mask_auth_token(url: &str) -> String {
    let needle = "&auth-token=";
    let Some(start) = url.find(needle) else {
        return url.to_string();
    };
    let value_start = start + needle.len();
    let value_end = url[value_start..]
        .find('&')
        .map(|n| value_start + n)
        .unwrap_or(url.len());
    let mut out = String::with_capacity(url.len());
    out.push_str(&url[..value_start]);
    out.push_str("***");
    out.push_str(&url[value_end..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::writer::operation::{AddTodoSpec, Operation};

    fn one_add_todo(title: &str) -> Operation {
        Operation::AddTodo(AddTodoSpec {
            title: title.into(),
            ..Default::default()
        })
    }

    #[test]
    fn build_url_includes_things_json_scheme_and_encoded_data() {
        let url = build_url(&[one_add_todo("Buy milk")], None);
        assert!(url.starts_with("things:///json?data="));
        // No auth-token when None.
        assert!(!url.contains("auth-token="));
        // Title should be encoded inside the data payload.
        assert!(url.contains("Buy%20milk") || url.contains("Buy%20milk"));
    }

    #[test]
    fn build_url_appends_auth_token_when_present() {
        let token = SecretString::new("abc 123/+&=");
        let url = build_url(&[one_add_todo("x")], Some(&token));
        assert!(url.contains("&auth-token="));
        // The token's special chars must be percent-encoded.
        assert!(url.contains("abc%20123%2F%2B%26%3D"));
    }

    #[test]
    fn mask_auth_token_redacts_segment() {
        let masked = mask_auth_token(
            "things:///json?data=%5B%5D&auth-token=supersecret",
        );
        assert_eq!(masked, "things:///json?data=%5B%5D&auth-token=***");
    }

    #[test]
    fn mask_auth_token_passes_through_when_absent() {
        let url = "things:///json?data=%5B%5D";
        assert_eq!(mask_auth_token(url), url);
    }
}
