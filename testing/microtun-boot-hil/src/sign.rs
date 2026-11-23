//! MCUboot signing with upstream `imgtool` and microtun-signer local keys,
//! plus envelope mutations.
//!
//! Arguments follow `microtun_boot_build::tools::mcuboot`, which the boards' Rust signing tools use;
//! the slot geometry comes from the board's [`Signing`]. Set `IMGTOOL` to
//! override the executable. HIL signs ordinary images at the actual DUT
//! security-counter floor; explicit signing arguments take precedence.

use std::{
    collections::HashMap,
    process::Command,
    sync::{Mutex, OnceLock},
};

use microtun_boot_build::tools::signer::{self, FIRMWARE_DEV_KEY, Signer};
use sha2::{Digest, Sha256};

use crate::Signing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// The local development key `microtun-boot-local-p256`, trusted by the test
    /// builds (see [`trusted_public_key`]).
    Dev,
    /// A freshly generated P-256 key the bootloader does not trust.
    Untrusted,
}

/// A throwaway key in a per-process local store.
const UNTRUSTED_KEY_ID: &str = "microtun-boot-hil-untrusted-p256";

impl Key {
    /// The microtun-signer key this fixture key signs with. The development
    /// key comes from the local store whatever the shell's signer settings
    /// (only a `local:` MICROTUN_SIGNER_URL can choose the store).
    pub fn signer(self) -> crate::Result<Signer> {
        match self {
            Key::Dev => Ok(Signer::local(FIRMWARE_DEV_KEY.key_id)),
            Key::Untrusted => untrusted_signer(),
        }
    }

    /// The key's SPKI PEM, fetched once per test process.
    pub fn public_pem(self) -> crate::Result<String> {
        static PEMS: [OnceLock<Result<String, String>>; 2] = [const { OnceLock::new() }; 2];
        PEMS[self as usize]
            .get_or_init(|| {
                self.signer()
                    .and_then(|signer| signer.public_key().map_err(crate::Error::external))
                    .map_err(|e| e.to_string())
            })
            .clone()
            .map_err(Into::into)
    }
}

/// The public key the HIL bootloader builds trust (`MICROTUN_BOOT_PUBKEYS`).
pub fn trusted_public_key() -> crate::Result<String> {
    Key::Dev.public_pem()
}

fn imgtool() -> String {
    std::env::var("IMGTOOL").unwrap_or_else(|_| "imgtool".into())
}

/// Read the current security epoch from the device under test, not an
/// independently configured value. Cache it for ordinary fixtures so every
/// image in this test process uses the same floor. An incrementing test reads
/// its dedicated QEMU board separately.
pub fn initial_security_counter() -> crate::Result<u32> {
    static FLOOR: OnceLock<Result<u32, String>> = OnceLock::new();
    match FLOOR.get_or_init(|| read_initial_security_counter().map_err(|e| e.to_string())) {
        Ok(value) => Ok(*value),
        Err(error) => Err(error.clone().into()),
    }
}

fn read_initial_security_counter() -> crate::Result<u32> {
    #[cfg(feature = "qemu")]
    {
        // Ordinary QEMU boards start with freshly initialized eFuse storage.
        crate::qemu::VirtualBoard::new(0)?.security_counter()
    }
    #[cfg(any(feature = "esp32c3-supermini", feature = "rp2350-pico2"))]
    {
        // No firmware compilation, image composition, flashing, or bootloader
        // output: both physical backends read the chip's fuses directly.
        #[cfg(feature = "esp32c3-supermini")]
        let board = &crate::esp32c3::BOARD;
        #[cfg(feature = "rp2350-pico2")]
        let board = &crate::rp2350::BOARD;
        let port_var = board.env("PORT");
        let port = std::env::var(&port_var)
            .map_err(|_| format!("set {port_var} to {}", board.console.port_hint))?;
        let _lock = crate::board_lock::BoardLock::acquire(board, &port)?;
        #[cfg(feature = "esp32c3-supermini")]
        return crate::esp32c3::security_counter::read();
        #[cfg(feature = "rp2350-pico2")]
        return crate::rp2350::probe::read_security_counter();
    }
    #[cfg(not(any(
        feature = "esp32c3-supermini",
        feature = "rp2350-pico2",
        feature = "qemu"
    )))]
    Err("select a HIL board feature to read the security counter".into())
}

fn untrusted_signer() -> crate::Result<Signer> {
    static STORE: OnceLock<Result<(tempfile::TempDir, Signer), String>> = OnceLock::new();
    match STORE.get_or_init(|| {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let url = format!("local:{}", dir.path().join("store").display());
        // Generate a fixture-only key directly with microtun-signer; do not
        // expose key lifecycle operations through the microtun boot host tools.
        let status = Command::new(signer::program())
            .args([
                "--url",
                url.as_str(),
                "local",
                "init",
                "--key-id",
                UNTRUSTED_KEY_ID,
                "--algorithm",
                "p256",
            ])
            .status()
            .map_err(|e| format!("cannot run microtun-signer local init: {e}"))?;
        if !status.success() {
            return Err(format!("microtun-signer local init failed: {status}"));
        }
        let signer = Signer {
            url,
            key_id: UNTRUSTED_KEY_ID.into(),
            github_login: false,
        };
        Ok((dir, signer))
    }) {
        Ok((_, signer)) => Ok(signer.clone()),
        Err(error) => Err(error.clone().into()),
    }
}

#[derive(Clone, Debug)]
pub struct SignOptions {
    pub version: String,
    pub key: Key,
    /// `--pad --confirm`: a pre-confirmed image padded to the slot size.
    pub confirm: bool,
    /// Further imgtool arguments, e.g. `--security-counter 3` or `--rom-fixed`.
    pub extra: Vec<String>,
}

impl SignOptions {
    pub fn new(version: &str) -> Self {
        Self {
            version: version.into(),
            key: Key::Dev,
            confirm: false,
            extra: Vec::new(),
        }
    }
    pub fn key(mut self, key: Key) -> Self {
        self.key = key;
        self
    }
    pub fn confirmed(mut self) -> Self {
        self.confirm = true;
        self
    }
    /// Override the default HIL epoch for this image.
    pub fn security_counter(self, value: u32) -> Self {
        self.arg("--security-counter").arg(value.to_string())
    }
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.extra.push(arg.into());
        self
    }
}

/// `imgtool sign` a payload for the board's slots.
pub fn sign(signing: &Signing, payload: &[u8], opts: &SignOptions) -> crate::Result<Vec<u8>> {
    let counter = if opts
        .extra
        .iter()
        .any(|arg| arg == "--security-counter" || arg.starts_with("--security-counter="))
    {
        None
    } else {
        Some(initial_security_counter()?)
    };
    let Signing {
        header_size,
        slot_size,
        sector,
        rom_fixed,
        vid,
        cid,
    } = *signing;
    // Reuse ordinary signed fixtures: randomized ECDSA signatures would
    // otherwise force an identical image's last sector to be reprogrammed.
    // Extra arguments can refer to files; leave those calls uncached. The
    // Geometry and the default epoch are part of the key so cached images
    // cannot cross boards or security-counter settings.
    type CacheKey = (
        [u8; 32],
        [u8; 32],
        String,
        bool,
        String,
        [u32; 3],
        Option<u32>,
        Option<u32>,
        &'static str,
        &'static str,
    );
    static CACHE: OnceLock<Mutex<HashMap<CacheKey, Vec<u8>>>> = OnceLock::new();
    let key = (
        Sha256::digest(payload).into(),
        Sha256::digest(opts.key.public_pem()?).into(),
        opts.version.clone(),
        opts.confirm,
        imgtool(),
        [header_size, slot_size, sector],
        rom_fixed,
        counter,
        vid,
        cid,
    );
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if opts.extra.is_empty()
        && let Some(image) = cache.get(&key)
    {
        return Ok(image.clone());
    }
    let geometry = microtun_boot_build::tools::mcuboot::Geometry {
        header: header_size,
        slot: slot_size,
        erase: sector,
        write: 4,
        rom_fixed,
        class: cid,
    };
    let mut options = microtun_boot_build::tools::mcuboot::SignOptions {
        signer: opts.key.signer()?,
        version: opts.version.clone(),
        counter: counter.unwrap_or(0).to_string(),
        vendor: vid.into(),
        class: cid.into(),
        extra: opts.extra.clone(),
    };
    if opts.confirm {
        options.extra.extend(["--pad".into(), "--confirm".into()]);
    }
    let image = microtun_boot_build::tools::mcuboot::sign(payload, geometry, &options)
        .map_err(crate::Error::external)?;
    if opts.extra.is_empty() {
        cache.insert(key, image.clone());
    }
    Ok(image)
}

// ----- envelope mutations ----------------------------------------------------

fn u16_at(b: &[u8], off: usize) -> usize {
    u16::from_le_bytes([b[off], b[off + 1]]) as usize
}

/// Offsets of every unprotected TLV as (kind, value offset, length).
pub fn unprotected_tlvs(image: &[u8]) -> Vec<(u16, usize, usize)> {
    let hdr = u16_at(image, 8);
    let prot = u16_at(image, 10);
    let body = u32::from_le_bytes(image[12..16].try_into().unwrap()) as usize;
    let start = hdr + body + prot;
    assert_eq!(u16_at(image, start), 0x6907, "unprotected TLV info magic");
    let end = start + u16_at(image, start + 2);
    let mut out = Vec::new();
    let mut at = start + 4;
    while at + 4 <= end {
        let kind = u16_at(image, at) as u16;
        let len = u16_at(image, at + 2);
        out.push((kind, at + 4, len));
        at += 4 + len;
    }
    out
}

/// Flip a bit inside the ECDSA signature's `r` integer (DER stays valid).
pub fn corrupt_signature(mut image: Vec<u8>) -> Vec<u8> {
    let (_, at, _) = *unprotected_tlvs(&image)
        .iter()
        .find(|(k, _, _)| *k == 0x22)
        .expect("ECDSA_SIG TLV");
    image[at + 6] ^= 0x01;
    image
}

/// Flip one bit of the signed payload `offset` bytes after the MCUboot header
/// (whose size the image declares).
pub fn flip_payload_bit(mut image: Vec<u8>, offset: usize) -> Vec<u8> {
    let header = u16_at(&image, 8);
    image[header + offset] ^= 0x01;
    image
}

/// Replace the signed hash TLV with zeros (signature then fails too).
pub fn corrupt_hash_tlv(mut image: Vec<u8>) -> Vec<u8> {
    let (_, at, len) = *unprotected_tlvs(&image)
        .iter()
        .find(|(k, _, _)| *k == 0x10)
        .expect("SHA256 TLV");
    image[at..at + len].fill(0);
    image
}
