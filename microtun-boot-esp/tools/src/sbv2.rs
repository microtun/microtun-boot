//! ESP ROM RSA-3072/PSS/SHA256 signature sectors, including Montgomery fields.
use anyhow::{Context, Result, ensure};
use microtun_boot_build::tools::signer::Signer;
use rsa::{
    BoxedUint, PublicKey,
    pkcs1::DecodeRsaPublicKey,
    pkcs8::DecodePublicKey,
    sha2::{Digest, Sha256},
    traits::PublicKeyParts,
};

const SECTOR: usize = 4096;
const BLOCK: usize = 1216;
const KEY_START: usize = 36;
const SIGNATURE_START: usize = 812;
const CRC: usize = 1196;

/// An RSA-3072 public key, as SPKI (`microtun-signer public-key`) or PKCS#1 PEM.
fn public(pem: &str) -> Result<PublicKey> {
    let key = PublicKey::from_public_key_pem(pem)
        .or_else(|_| PublicKey::from_pkcs1_pem(pem))
        .context("SBv2 needs an RSA public key PEM")?;
    ensure!(
        key.n().bit_width_vartime() == 3072,
        "SBv2 requires RSA-3072"
    );
    Ok(key)
}
fn key_fields(key: &PublicKey) -> Result<Vec<u8>> {
    let mut result = key.n().to_le_bytes().into_vec();
    ensure!(result.len() == 384, "SBv2 requires RSA-3072");
    let e = key.e().to_le_bytes_trimmed_vartime();
    ensure!(e.len() <= 4, "RSA exponent exceeds u32");
    let mut exponent = [0; 4];
    exponent[..e.len()].copy_from_slice(&e);
    result.extend_from_slice(&exponent);

    // ESP ROM stores R^2 mod N for R = 2^3072. The 0.10 API uses
    // fixed-precision BoxedUint in place of arbitrary-precision BigUint.
    // Repeated modular doubling avoids constructing a 6145-bit temporary.
    let mut rr = BoxedUint::one_with_precision(key.n_bits_precision());
    for _ in 0..6144 {
        rr = rr.double_mod(key.n());
    }
    result.extend_from_slice(&rr.to_le_bytes());
    let n = u32::from_le_bytes(result[..4].try_into()?);
    // Newton iteration doubles the number of correct low bits each step.
    let mut inv = 1u32;
    for _ in 0..5 {
        inv = inv.wrapping_mul(2u32.wrapping_sub(n.wrapping_mul(inv)));
    }
    result.extend_from_slice(&inv.wrapping_neg().to_le_bytes());
    Ok(result)
}
/// Sign with microtun-signer (a local development key or a signer service).
/// The signature is checked locally against the signer's public key before
/// any signed image is returned to the caller.
pub fn sign(image: &[u8], signer: &Signer) -> Result<Vec<u8>> {
    ensure!(!image.is_empty(), "empty ROM image");
    let public_pem = signer.public_key()?;
    sign_with(image, &public_pem, |digest| signer.sign_digest(digest))
}

/// Build the signature sector for `public_pem`, with `sign` producing the
/// raw 384-byte RSA-PSS (SHA-256, 32-byte salt) signature of the digest.
pub fn sign_with(
    image: &[u8],
    public_pem: &str,
    sign: impl FnOnce(&[u8]) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    ensure!(!image.is_empty(), "empty ROM image");
    let key = public(public_pem)?;
    let mut signed = image.to_vec();
    signed.resize(image.len().next_multiple_of(SECTOR), 0xff);
    let digest = Sha256::digest(&signed);
    let mut block = vec![0; BLOCK];
    block[..4].copy_from_slice(&[0xe7, 2, 0, 0]);
    block[4..36].copy_from_slice(&digest);
    block[KEY_START..SIGNATURE_START].copy_from_slice(&key_fields(&key)?);
    let mut signature = sign(&digest)?;
    ensure!(
        signature.len() == 384,
        "SBv2 requires a 384-byte RSA-3072 signature"
    );
    key.pss_verify_prehash::<Sha256>(&digest, &signature)
        .context("SBv2 signature does not match the signer's public key")?;
    signature.reverse();
    block[SIGNATURE_START..CRC].copy_from_slice(&signature);
    let crc = crc32fast::hash(&block[..CRC]);
    block[CRC..CRC + 4].copy_from_slice(&crc.to_le_bytes());
    signed.extend_from_slice(&block);
    signed.resize(signed.len() + SECTOR - BLOCK, 0xff);
    verify(&signed, public_pem)?;
    Ok(signed)
}

/// Check the two stages cannot accidentally be signed with different ROM keys.
pub fn verify_same_key(a: &[u8], b: &[u8]) -> Result<()> {
    ensure!(
        a.len() >= SECTOR && b.len() >= SECTOR,
        "invalid ROM signed stage size"
    );
    let a = &a[a.len() - SECTOR..];
    let b = &b[b.len() - SECTOR..];
    ensure!(
        a[KEY_START..SIGNATURE_START] == b[KEY_START..SIGNATURE_START],
        "ESP IDF and microtun boot stages have different RSA signing keys"
    );
    Ok(())
}

/// Verify the ESP ROM signature using the public RSA key embedded in the
/// signature sector. This verifies integrity, not whether the device trusts
/// that key in its eFuses.
pub fn verify_embedded(signed: &[u8]) -> Result<()> {
    ensure!(signed.len() >= 2 * SECTOR, "invalid SBv2 size");
    let sector = &signed[signed.len() - SECTOR..];
    let modulus = BoxedUint::from_le_slice(&sector[KEY_START..KEY_START + 384], 3072)
        .context("invalid embedded RSA modulus")?;
    let exponent = BoxedUint::from(u32::from_le_bytes(
        sector[KEY_START + 384..KEY_START + 388].try_into()?,
    ));
    let key = PublicKey::new(modulus, exponent).context("invalid embedded RSA key")?;
    ensure!(
        key.n().bit_width_vartime() == 3072,
        "SBv2 requires RSA-3072"
    );
    verify_with_key(signed, &key)
}

/// Verify against a trusted RSA-3072 public key PEM.
pub fn verify(signed: &[u8], pem: &str) -> Result<()> {
    verify_with_key(signed, &public(pem)?)
}

fn verify_with_key(signed: &[u8], key: &PublicKey) -> Result<()> {
    ensure!(
        signed.len() >= 2 * SECTOR && signed.len().is_multiple_of(SECTOR),
        "invalid SBv2 size"
    );
    let (image, sector) = signed.split_at(signed.len() - SECTOR);
    ensure!(
        sector[..4] == [0xe7, 2, 0, 0]
            && sector[CRC + 4..BLOCK].iter().all(|b| *b == 0)
            && sector[BLOCK..].iter().all(|b| *b == 0xff),
        "invalid SBv2 header/padding"
    );
    ensure!(
        microtun_boot_build::tools::word(sector, CRC)? == crc32fast::hash(&sector[..CRC]),
        "SBv2 CRC mismatch"
    );
    let digest = Sha256::digest(image);
    ensure!(sector[4..36] == digest[..], "SBv2 image digest mismatch");
    ensure!(
        sector[KEY_START..SIGNATURE_START] == key_fields(key)?,
        "SBv2 trusted key mismatch"
    );
    let mut signature = sector[SIGNATURE_START..CRC].to_vec();
    signature.reverse();
    key.pss_verify_prehash::<Sha256>(&digest, &signature)
        .context("SBv2 RSA-PSS mismatch")
}

#[cfg(test)]
pub(crate) mod test_key {
    use anyhow::Result;
    use rsa::{Generate, PrivateKey, pkcs8::EncodePublicKey, sha2::Sha256};
    use std::sync::OnceLock;

    /// One in-process RSA-3072 key per test binary, standing in for the signer.
    fn key() -> &'static PrivateKey {
        static KEY: OnceLock<PrivateKey> = OnceLock::new();
        KEY.get_or_init(PrivateKey::generate)
    }

    pub fn public_pem() -> String {
        key()
            .to_public_key()
            .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
            .unwrap()
    }

    pub fn sign(digest: &[u8]) -> Result<Vec<u8>> {
        Ok(key().pss_sign_prehash::<Sha256>(digest)?)
    }

    pub fn signed(image: &[u8]) -> Vec<u8> {
        super::sign_with(image, &public_pem(), sign).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_sector_verifies_and_rejects_tampering() {
        let pem = test_key::public_pem();
        let mut signed = test_key::signed(&[0xe9; 0x1234]);
        assert_eq!(signed.len(), 0x2000 + SECTOR);
        verify(&signed, &pem).unwrap();
        verify_embedded(&signed).unwrap();
        signed[0x10] ^= 1;
        assert!(verify(&signed, &pem).is_err());
        assert!(verify_embedded(&signed).is_err());
    }

    #[test]
    fn mismatching_signature_is_rejected_before_assembly() {
        let pem = test_key::public_pem();
        assert!(sign_with(&[0xe9; 16], &pem, |_| Ok(vec![0; 384])).is_err());
        assert!(sign_with(&[0xe9; 16], &pem, |_| Ok(vec![0; 383])).is_err());
    }
}
