//! Fixed-size cursor claims contain hashes, never paths or display labels.
use crate::{ErrorCode, StoreResult};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"TokenPulse/page-cursor/v1\0";
const CLAIM_BYTES: usize = 1 + 16 + 32 + 32;
const TOKEN_BYTES: usize = CLAIM_BYTES + 32;
const TOKEN_CHARS: usize = TOKEN_BYTES.div_ceil(3) * 4 - 1;
type HmacSha256 = Hmac<Sha256>;

/// Includes the trusted window identity, full request, sort and price basis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryBinding([u8; 32]);
impl QueryBinding {
    pub fn new(owner: &str, request: &impl Serialize) -> StoreResult<Self> {
        if owner.is_empty() || owner.len() > 64 || owner.chars().any(char::is_control) {
            return Err(ErrorCode::InvalidQuery.into());
        }
        let encoded = serde_json::to_vec(&(owner, request))?;
        if encoded.len() > 128 * 1024 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        Ok(Self(Sha256::digest(encoded).into()))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorClaims {
    pub snapshot_id: [u8; 16],
    pub position_hash: [u8; 32],
}
impl CursorClaims {
    /// The actual last tuple stays in the lease. Only its digest enters the cursor.
    pub fn new(snapshot_id: [u8; 16], last_tuple: &impl Serialize) -> StoreResult<Self> {
        let encoded = serde_json::to_vec(last_tuple)?;
        if encoded.len() > 1024 {
            return Err(ErrorCode::InvalidQuery.into());
        }
        Ok(Self {
            snapshot_id,
            position_hash: Sha256::digest(encoded).into(),
        })
    }
}
pub struct CursorSigner {
    key: [u8; 32],
}
impl CursorSigner {
    /// A fresh OS random key for every manager/process. Never persisted.
    pub fn new() -> StoreResult<Self> {
        let mut key = [0; 32];
        getrandom::fill(&mut key).map_err(|_| ErrorCode::DbWriteFailed)?;
        Ok(Self { key })
    }
    pub fn new_snapshot_id(&self) -> StoreResult<[u8; 16]> {
        let mut id = [0; 16];
        getrandom::fill(&mut id).map_err(|_| ErrorCode::DbWriteFailed)?;
        Ok(id)
    }
    pub fn issue(&self, binding: &QueryBinding, claims: CursorClaims) -> String {
        let mut bytes = [0; TOKEN_BYTES];
        bytes[0] = 1;
        bytes[1..17].copy_from_slice(&claims.snapshot_id);
        bytes[17..49].copy_from_slice(&binding.0);
        bytes[49..CLAIM_BYTES].copy_from_slice(&claims.position_hash);
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("fixed HMAC key");
        mac.update(DOMAIN);
        mac.update(&bytes[..CLAIM_BYTES]);
        bytes[CLAIM_BYTES..].copy_from_slice(&mac.finalize().into_bytes());
        URL_SAFE_NO_PAD.encode(bytes)
    }
    /// Authenticate before reading claims or looking up a snapshot.
    pub fn read(&self, token: &str, binding: &QueryBinding) -> StoreResult<CursorClaims> {
        let invalid = || crate::StoreError::from(ErrorCode::CursorInvalid);
        if token.len() != TOKEN_CHARS {
            return Err(invalid());
        }
        let bytes = URL_SAFE_NO_PAD.decode(token).map_err(|_| invalid())?;
        if bytes.len() != TOKEN_BYTES || bytes[0] != 1 {
            return Err(invalid());
        }
        let mut mac = HmacSha256::new_from_slice(&self.key).expect("fixed HMAC key");
        mac.update(DOMAIN);
        mac.update(&bytes[..CLAIM_BYTES]);
        mac.verify_slice(&bytes[CLAIM_BYTES..])
            .map_err(|_| invalid())?;
        if bytes[17..49] != binding.0 {
            return Err(invalid());
        }
        Ok(CursorClaims {
            snapshot_id: bytes[1..17].try_into().map_err(|_| invalid())?,
            position_hash: bytes[49..CLAIM_BYTES].try_into().map_err(|_| invalid())?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn signed_cursor_binds_snapshot_exact_large_position_request_and_window_without_exposing_labels()
     {
        let signer = CursorSigner::new().unwrap();
        let request = json!({"filter":{"project":"project-id","label":"E:/private/project"},"sort":"total_desc","price_basis":"event_time"});
        let binding = QueryBinding::new("main", &request).unwrap();
        let claims = CursorClaims::new(
            [7; 16],
            &("9007199254740993", "session-id", "E:/private/project"),
        )
        .unwrap();
        let token = signer.issue(&binding, claims);
        assert_eq!(token.len(), 151);
        assert_eq!(signer.read(&token, &binding).unwrap(), claims);
        let payload = URL_SAFE_NO_PAD.decode(&token).unwrap();
        assert!(!payload.windows(b"private".len()).any(|w| w == b"private"));
        assert!(
            !payload
                .windows(b"9007199254740993".len())
                .any(|w| w == b"9007199254740993")
        );
        assert_ne!(
            claims.position_hash,
            CursorClaims::new(
                [7; 16],
                &("9007199254740992", "session-id", "E:/private/project")
            )
            .unwrap()
            .position_hash
        );
        assert_ne!(
            claims.position_hash,
            CursorClaims::new(
                [7; 16],
                &("9007199254740993", "another-session", "E:/private/project")
            )
            .unwrap()
            .position_hash
        );
        for changed in [
            json!({"filter":{"project":"other-id","label":"E:/private/project"},"sort":"total_desc","price_basis":"event_time"}),
            json!({"filter":{"project":"project-id","label":"E:/private/project"},"sort":"name_asc","price_basis":"event_time"}),
            json!({"filter":{"project":"project-id","label":"E:/private/project"},"sort":"total_desc","price_basis":"specified_time"}),
        ] {
            assert_eq!(
                signer
                    .read(&token, &QueryBinding::new("main", &changed).unwrap())
                    .unwrap_err()
                    .code,
                ErrorCode::CursorInvalid
            );
        }
        assert_eq!(
            signer
                .read(&token, &QueryBinding::new("mini", &request).unwrap())
                .unwrap_err()
                .code,
            ErrorCode::CursorInvalid
        );
    }
    #[test]
    fn any_changed_claim_or_signature_byte_is_rejected_and_a_new_process_key_invalidates_old_tokens()
     {
        let signer = CursorSigner::new().unwrap();
        let binding = QueryBinding::new("main", &json!({"query":"fixture"})).unwrap();
        let token = signer.issue(&binding, CursorClaims::new([3; 16], &(42, "id")).unwrap());
        let payload = URL_SAFE_NO_PAD.decode(&token).unwrap();
        for index in 0..payload.len() {
            let mut altered = payload.clone();
            altered[index] ^= 1;
            assert_eq!(
                signer
                    .read(&URL_SAFE_NO_PAD.encode(altered), &binding)
                    .unwrap_err()
                    .code,
                ErrorCode::CursorInvalid
            );
        }
        assert_eq!(
            CursorSigner::new()
                .unwrap()
                .read(&token, &binding)
                .unwrap_err()
                .code,
            ErrorCode::CursorInvalid
        );
    }
    #[test]
    fn malformed_oversized_trailing_and_noncanonical_cursor_encodings_are_rejected() {
        let signer = CursorSigner::new().unwrap();
        let binding = QueryBinding::new("main", &()).unwrap();
        let token = signer.issue(&binding, CursorClaims::new([1; 16], &()).unwrap());
        for invalid in [
            String::new(),
            "x".repeat(100_000),
            format!("{token}="),
            token[..150].to_owned(),
            "!".repeat(151),
        ] {
            assert_eq!(
                signer.read(&invalid, &binding).unwrap_err().code,
                ErrorCode::CursorInvalid
            );
        }
        let mut noncanonical = token.into_bytes();
        // 113 bytes leave two unused low bits in the final base64 digit.
        let last = noncanonical.last_mut().unwrap();
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let index = alphabet.iter().position(|c| c == last).unwrap();
        *last = alphabet[index | 1];
        assert_eq!(
            signer
                .read(std::str::from_utf8(&noncanonical).unwrap(), &binding)
                .unwrap_err()
                .code,
            ErrorCode::CursorInvalid
        );
    }
    #[test]
    fn identities_and_private_last_tuples_are_bounded_and_snapshot_ids_are_fresh() {
        for owner in ["", "bad\nowner", &"x".repeat(65)] {
            assert_eq!(
                QueryBinding::new(owner, &()).unwrap_err().code,
                ErrorCode::InvalidQuery
            );
        }
        assert_eq!(
            QueryBinding::new("main", &"x".repeat(128 * 1024))
                .unwrap_err()
                .code,
            ErrorCode::InvalidQuery
        );
        assert_eq!(
            CursorClaims::new([0; 16], &"x".repeat(1024))
                .unwrap_err()
                .code,
            ErrorCode::InvalidQuery
        );
        let signer = CursorSigner::new().unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..256 {
            assert!(seen.insert(signer.new_snapshot_id().unwrap()));
        }
    }
}
