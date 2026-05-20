//! `SecretString` — a tiny newtype that prevents accidental logging of
//! sensitive material. We roll our own to avoid pulling in the `secrecy`
//! crate for a use this small. Only `expose_secret()` returns the raw value.

#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the raw secret. Call sites must NEVER log the returned `&str`.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretString(***)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_does_not_expose_secret() {
        let s = SecretString::new("totally-secret-token");
        let dbg = format!("{:?}", s);
        assert!(!dbg.contains("totally-secret"));
        assert_eq!(dbg, "SecretString(***)");
    }

    #[test]
    fn expose_secret_returns_raw() {
        let s = SecretString::new("abc123");
        assert_eq!(s.expose_secret(), "abc123");
    }
}
