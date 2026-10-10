//! Capability token encryption and decryption for locally hosted Bazi charts.
//! Uses AES-256-GCM to encode user_id statelessly, skipping database lookups on chart access.

use aes_gcm::{
  Aes256Gcm, Key, Nonce,
  aead::{Aead, KeyInit},
};
use rand::Rng;

const NONCE_LEN: usize = 12;
const PAYLOAD_LEN: usize = 36; // 12-byte nonce + 8-byte ciphertext + 16-byte tag
pub const TOKEN_HEX_LEN: usize = 72; // 36 bytes encoded as hex

/// Encrypt a user ID into a capability token string using AES-256-GCM.
pub fn generate_chart_token(user_id: u64, secret: &[u8; 32]) -> Result<String, crate::models::AppError> {
  let cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(*secret));
  let nonce_bytes: [u8; NONCE_LEN] = rand::rng().random();
  let nonce = Nonce::from(nonce_bytes);

  let ciphertext_and_tag = cipher.encrypt(&nonce, user_id.to_be_bytes().as_ref()).map_err(|e| crate::models::AppError::Message(format!("Failed to encrypt chart token: {e}")))?;

  let mut payload = Vec::with_capacity(PAYLOAD_LEN);
  payload.extend_from_slice(&nonce_bytes);
  payload.extend_from_slice(&ciphertext_and_tag);

  Ok(hex::encode(payload))
}

/// Decrypt a capability token back to the owner's user ID without database lookups.
pub fn verify_chart_token(token: &str, secret: &[u8; 32]) -> Option<u64> {
  if token.len() != TOKEN_HEX_LEN || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
    return None;
  }

  let payload = hex::decode(token).ok()?;
  if payload.len() != PAYLOAD_LEN {
    return None;
  }

  let (nonce_bytes, ciphertext_and_tag) = payload.split_at(NONCE_LEN);
  let nonce_array: [u8; NONCE_LEN] = nonce_bytes.try_into().ok()?;
  let cipher = Aes256Gcm::new(&Key::<Aes256Gcm>::from(*secret));
  let nonce = Nonce::from(nonce_array);

  let plaintext = cipher.decrypt(&nonce, ciphertext_and_tag).ok()?;
  if plaintext.len() != 8 {
    return None;
  }

  let bytes: [u8; 8] = plaintext.try_into().ok()?;
  Some(u64::from_be_bytes(bytes))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_chart_token_round_trip() {
    let secret = [42u8; 32];
    let user_id = 9876543210u64;

    let token = generate_chart_token(user_id, &secret).expect("encryption succeeds");
    assert_eq!(token.len(), TOKEN_HEX_LEN);

    let recovered = verify_chart_token(&token, &secret);
    assert_eq!(recovered, Some(user_id));
  }

  #[test]
  fn test_chart_token_rejects_tampering() {
    let secret = [42u8; 32];
    let user_id = 12345u64;

    let mut token = generate_chart_token(user_id, &secret).expect("encryption succeeds");
    // Flip a character
    let last_char = if token.ends_with('0') { '1' } else { '0' };
    token.pop();
    token.push(last_char);

    assert_eq!(verify_chart_token(&token, &secret), None);
  }

  #[test]
  fn test_chart_token_wrong_key_fails() {
    let secret_a = [1u8; 32];
    let secret_b = [2u8; 32];
    let user_id = 12345u64;

    let token = generate_chart_token(user_id, &secret_a).expect("encryption succeeds");
    assert_eq!(verify_chart_token(&token, &secret_b), None);
  }

  #[test]
  fn test_chart_token_rejects_invalid_lengths() {
    let secret = [42u8; 32];
    assert_eq!(verify_chart_token("abcd", &secret), None);
    assert_eq!(verify_chart_token(&"0".repeat(64), &secret), None);
    assert_eq!(verify_chart_token(&"0".repeat(73), &secret), None);
  }
}
