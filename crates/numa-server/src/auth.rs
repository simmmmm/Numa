use std::sync::Arc;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use sha2::{Digest, Sha256};

use crate::store::Account;
use crate::{Fail, Server};

pub const COOKIE: &str = "numa_session";

pub fn hash(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(password.as_bytes(), &salt).map(|hash| hash.to_string()).map_err(|err| err.to_string())
}

pub fn verify(password: &str, hash: Option<&str>) -> bool {
    static NOBODY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let nobody = NOBODY.get_or_init(|| self::hash("nobody").unwrap_or_default());
    let known = hash.is_some();
    let Ok(parsed) = PasswordHash::new(hash.unwrap_or(nobody)) else { return false };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok() && known
}

pub fn token() -> (String, String) {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = hex(&bytes);
    let digest = digest(&token);
    (token, digest)
}

pub fn digest(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

pub fn sha256_of(path: &std::path::Path) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn valid_name(name: &str) -> bool {
    let fine = |c: char| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_';
    !name.is_empty() && name.chars().count() <= 40 && name.chars().all(fine) && name.trim() == name
}

pub fn presented(headers: &HeaderMap) -> Option<String> {
    if let Some(bearer) = headers.get("authorization").and_then(|value| value.to_str().ok()).and_then(|value| value.strip_prefix("Bearer ")) {
        return Some(bearer.trim().to_string());
    }
    headers
        .get_all("cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(COOKIE).and_then(|rest| rest.strip_prefix('=')).map(str::to_string))
}

pub struct Signed(pub Account);

impl FromRequestParts<Arc<Server>> for Signed {
    type Rejection = Fail;

    async fn from_request_parts(parts: &mut Parts, server: &Arc<Server>) -> Result<Self, Fail> {
        presented(&parts.headers)
            .and_then(|token| server.store.session(&digest(&token)))
            .map(Signed)
            .ok_or_else(|| Fail(StatusCode::UNAUTHORIZED, "Sign in first.".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_and_names() {
        let hashed = hash("correct horse").unwrap();
        assert!(verify("correct horse", Some(&hashed)));
        assert!(!verify("wrong", Some(&hashed)));
        assert!(!verify("nobody", None), "an unknown name never signs in");
        assert!(valid_name("Frédérique") && valid_name("Tijmen de Vries"));
        for bad in ["", "..", ".hidden", "a/b", "a\\b", " lead", "x".repeat(41).as_str()] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        let (token, kept) = token();
        assert_eq!(token.len(), 64);
        assert_eq!(digest(&token), kept);
    }
}
