use k256::ecdsa::{
    signature::{Signer, Verifier},
    Signature, SigningKey, VerifyingKey,
};
use k256::schnorr::VerifyingKey as SchnorrVk;
use mini_social_feed_app::AppError;
use rand_core::OsRng;

pub fn generate_keypair() -> (SigningKey, VerifyingKey) {
    let sk = SigningKey::random(&mut OsRng);
    let vk = VerifyingKey::from(&sk);
    (sk, vk)
}

pub fn public_key_to_hex(vk: &VerifyingKey) -> String {
    hex::encode(vk.to_encoded_point(true).as_bytes())
}

pub fn public_key_from_hex(hex_str: &str) -> Result<VerifyingKey, AppError> {
    let bytes =
        hex::decode(hex_str).map_err(|_| AppError::BadRequest("invalid public key hex".into()))?;
    VerifyingKey::from_sec1_bytes(&bytes)
        .map_err(|_| AppError::BadRequest("bad sec1 / compressed key".into()))
}

pub fn sign(sk: &SigningKey, message: &[u8]) -> Signature {
    sk.sign(message)
}

pub fn verify(vk: &VerifyingKey, message: &[u8], sig: &Signature) -> bool {
    vk.verify(message, sig).is_ok()
}

pub fn signature_to_hex(sig: &Signature) -> String {
    hex::encode(sig.to_bytes())
}

pub fn signature_from_hex(hex_str: &str) -> Result<Signature, AppError> {
    let bytes =
        hex::decode(hex_str).map_err(|_| AppError::BadRequest("invalid signature hex".into()))?;
    Signature::from_slice(&bytes)
        .map_err(|_| AppError::BadRequest("invalid signature encoding".into()))
}

pub fn signing_key_from_hex(hex_str: &str) -> Result<SigningKey, AppError> {
    let bytes = hex::decode(hex_str).map_err(|_| AppError::BadRequest("invalid key hex".into()))?;
    SigningKey::from_slice(&bytes).map_err(|_| AppError::BadRequest("invalid signing key".into()))
}

pub fn generate_challenge() -> [u8; 32] {
    use rand_core::RngCore;
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    buf
}

// ---------------------------------------------------------------------------
// Password hashing (argon2id)
// ---------------------------------------------------------------------------

use argon2::{
    password_hash::{rand_core::OsRng as ArgonOsRng, PasswordHasher, SaltString},
    Argon2, PasswordHash, PasswordVerifier,
};

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut ArgonOsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    let parsed = PasswordHash::new(hash).map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

// ---------------------------------------------------------------------------
// Schnorr (Nostr) verification
// ---------------------------------------------------------------------------

/// Verify a Schnorr signature against an x-only public key derived from a
/// 33-byte compressed secp256k1 key.
pub fn schnorr_verify(pubkey_hex: &str, msg: &[u8], sig_hex: &str) -> Result<(), AppError> {
    let sig_bytes =
        hex::decode(sig_hex).map_err(|_| AppError::BadRequest("invalid signature hex".into()))?;

    if sig_bytes.len() != 64 {
        return Err(AppError::BadRequest(
            "schnorr signature must be 64 bytes".into(),
        ));
    }

    let pk_bytes =
        hex::decode(pubkey_hex).map_err(|_| AppError::BadRequest("invalid pubkey hex".into()))?;

    // Accept both 33-byte compressed and 32-byte x-only
    let x_only: &[u8] = if pk_bytes.len() == 33 {
        &pk_bytes[1..]
    } else if pk_bytes.len() == 32 {
        &pk_bytes
    } else {
        return Err(AppError::BadRequest("pubkey must be 32 or 33 bytes".into()));
    };

    let x_arr: &[u8; 32] = x_only
        .try_into()
        .map_err(|_| AppError::BadRequest("invalid pubkey length".into()))?;

    let vk = SchnorrVk::from_bytes(x_arr)
        .map_err(|_| AppError::BadRequest("invalid schnorr public key".into()))?;

    let sig = k256::schnorr::Signature::try_from(sig_bytes.as_slice())
        .map_err(|_| AppError::BadRequest("invalid schnorr signature".into()))?;

    vk.verify_raw(msg, &sig)
        .map_err(|_| AppError::BadRequest("schnorr verification failed".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schnorr_sign_and_verify_works() {
        let sk = k256::schnorr::SigningKey::random(&mut OsRng);
        let vk = sk.verifying_key();
        let xonly = hex::encode(vk.to_bytes());

        let msg = b"hello world";
        let aux = [0u8; 32];
        let sig = sk.sign_raw(msg, &aux).unwrap();
        let sig_hex = hex::encode(sig.to_bytes());

        schnorr_verify(&xonly, msg, &sig_hex).unwrap();
    }
}
