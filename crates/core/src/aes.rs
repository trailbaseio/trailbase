use aes_gcm_siv::aead::{Aead, Generate, Payload};
use aes_gcm_siv::aes::cipher::InOutBuf;
use aes_gcm_siv::{AeadInOut, Aes256GcmSiv, Key, KeyInit, Nonce};
use rand::Rng;

type Cipher = Aes256GcmSiv;
pub type KeyType = Key<Cipher>;

/// Encrypts the cookie's value with authenticated encryption providing
/// confidentiality, integrity, and authenticity.
pub fn encrypt(
  key: &KeyType,
  associated_data: &[u8],
  data: &[u8],
) -> Result<Vec<u8>, &'static str> {
  // Create a buffer to hold the [nonce | enc(payload) | tag].
  let mut buffer = vec![0; NONCE_LEN + data.len() + TAG_LEN];

  // Split data into three: nonce, input/output, tag. Copy input.
  let (nonce, in_out) = buffer.split_at_mut(NONCE_LEN);
  let (in_out, tag) = in_out.split_at_mut(data.len());
  in_out.copy_from_slice(data);

  // Fill nonce piece with random data.
  let mut rng = rand::rng();
  rng.fill_bytes(nonce);

  let nonce: &Nonce = nonce.as_array::<NONCE_LEN>().ok_or("invalid nonce")?.into();

  // Perform the actual sealing operation, using the associated data to prevent value swapping.
  let cipher = Cipher::new(key);
  let aad_tag = cipher
    .encrypt_inout_detached(nonce, associated_data, InOutBuf::from(in_out))
    .map_err(|_| "encryption failure!")?;

  // Copy the tag into the tag piece.
  tag.copy_from_slice(&aad_tag);

  return Ok(buffer);
}

pub fn decrypt(
  key: &KeyType,
  associated_data: &[u8],
  cipher_text: &[u8],
) -> Result<Vec<u8>, &'static str> {
  // Expects a cipher_text to be [nonce | enc(payload) | tag].
  if cipher_text.len() < NONCE_LEN + TAG_LEN {
    return Err("input too short");
  }

  let (nonce, msg) = cipher_text.split_at(NONCE_LEN);
  let nonce: &Nonce = nonce.as_array::<NONCE_LEN>().ok_or("invalid nonce")?.into();

  // NOTE: We're not using the in-place variants like for encyrption, which results in more
  // allocations.
  let cipher = Cipher::new(key);
  return cipher
    .decrypt(
      nonce,
      Payload {
        msg,
        aad: associated_data,
      },
    )
    .map_err(|_| "invalid key/nonce/value: bad seal");
}

pub fn generate_random_key() -> KeyType {
  return KeyType::generate_from_rng(&mut rand::rng());
}

const NONCE_LEN: usize = 12;
const TAG_LEN: usize = 16;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_encryption() {
    let key = generate_random_key();
    let associated_data = b"test-tag";

    let payload = b"secret payload";
    let encrypted = encrypt(&key, associated_data, payload).unwrap();
    let decrypted = decrypt(&key, associated_data, &encrypted).unwrap();

    assert_eq!(payload, decrypted.as_slice());

    assert!(decrypt(&key, b"other", &encrypted).is_err());
  }
}
