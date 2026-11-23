//! Build-script helpers shared by the microtun boot platform crates and board
//! binaries, plus optional host image tooling. Nothing here runs on a target.
//! Build scripts use a `[build-dependencies]` entry with no features; board
//! tools and HIL enable `tools`, and CLI binaries also enable `cli`.
//!
//! * [`emit_public_keys`] embeds the trusted ECDSA P-256 keys as
//!   `$OUT_DIR/keys.rs`, by default the local development key from
//!   microtun-signer's local store.
//! * [`selected_chip`] and [`publish_chip`] (platform crates) and
//!   [`check_platform_chip`] / [`platform_chip`] (binaries) make sure a binary
//!   links the platform crate built for its board's chip.
//! * [`configure_esp_application`] and [`configure_rp2350_application`] pass
//!   an application's linker arguments. The chip-only linker contract is
//!   exported by `microtun-boot-esp` / `microtun-boot-rp`. RP2350 MEMORY regions are
//!   generated here, in the application's own build script, from
//!   `chips/rp235x` and the board's maximum image size; ESP applications take
//!   theirs from `esp-hal` and need no board input.

#![forbid(unsafe_code)]

#[cfg(feature = "tools")]
pub mod tools;

mod platform;

pub use platform::{
    PlatformChip, check_platform_chip, platform_chip, publish_chip, require_target, selected_chip,
};

use std::{env, fs, path::PathBuf};

/// Cargo's `OUT_DIR` for the running build script.
pub fn out_dir() -> PathBuf {
    PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"))
}

/// Write `contents` to `$OUT_DIR/name` and return the path.
pub fn write_out(name: &str, contents: impl AsRef<[u8]>) -> PathBuf {
    let path = out_dir().join(name);
    fs::write(&path, contents).unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
    path
}

/// The signer URL that selects microtun-signer's local development keys
/// (optionally `local:/absolute/dir`), and the default signer URL.
pub const LOCAL_SIGNER_URL: &str = "local:";

/// Key ID of the local development P-256 key that signs MCUboot images and
/// that bootloader builds trust when `MICROTUN_BOOT_PUBKEYS` is unset.
pub const FIRMWARE_DEV_KEY_ID: &str = "microtun-boot-local-p256";

/// Selects the MCUboot firmware key ID, ahead of `MICROTUN_SIGNER_KEY_ID`.
pub const FIRMWARE_KEY_ID_ENV: &str = "MICROTUN_BOOT_KEY_ID";

/// Whether a microtun-signer URL selects local signing rather than a service.
pub fn is_local_signer_url(url: &str) -> bool {
    url.starts_with(LOCAL_SIGNER_URL)
}

/// Read the ECDSA P-256 public keys listed (comma separated) in
/// `MICROTUN_BOOT_PUBKEYS` and generate `$OUT_DIR/keys.rs` with
/// `pub const PUBLIC_KEYS: [[u8; 65]; N]`.
///
/// Accepted formats are SPKI PEM (`microtun-signer public-key`, or
/// `imgtool getpub -k <key> -e pem`) and raw (`imgtool getpub -e raw`).
/// Relative paths are relative to the package being built (Cargo runs build
/// scripts there).
///
/// When `MICROTUN_BOOT_PUBKEYS` is unset, the build trusts one key from
/// microtun-signer's local store: `MICROTUN_BOOT_KEY_ID`, `MICROTUN_SIGNER_KEY_ID`
/// or [`FIRMWARE_DEV_KEY_ID`], in the store `MICROTUN_SIGNER_URL` names
/// (default `local:`); the same key that signs applications by default. A build never takes a key from a signer *service*: with a
/// service URL configured, `MICROTUN_BOOT_PUBKEYS` must name the pinned key.
pub fn emit_public_keys() {
    const SPKI_PREFIX: [u8; 26] = [
        0x30, 0x59, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08,
        0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x03, 0x42, 0x00,
    ];

    for var in [
        "MICROTUN_BOOT_PUBKEYS",
        "MICROTUN_SIGNER",
        "MICROTUN_SIGNER_URL",
        FIRMWARE_KEY_ID_ENV,
        "MICROTUN_SIGNER_KEY_ID",
        "MICROTUN_SIGNER_LOCAL_DIR",
        "XDG_DATA_HOME",
        "HOME",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let paths: Vec<String> = match env::var("MICROTUN_BOOT_PUBKEYS") {
        Ok(spec) if !spec.trim().is_empty() => spec
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect(),
        _ => vec![local_public_key_file()],
    };

    let mut keys = Vec::new();
    for path in &paths {
        println!("cargo:rerun-if-changed={path}");
        let data = fs::read(path).unwrap_or_else(|e| {
            panic!("cannot read public key {path}: {e} (set MICROTUN_BOOT_PUBKEYS)")
        });
        let der = if data.starts_with(b"-----BEGIN") {
            let pem = pem::parse(&data).unwrap_or_else(|e| panic!("{path}: invalid PEM: {e}"));
            if pem.tag() != "PUBLIC KEY" {
                panic!(
                    "{path}: expected a PUBLIC KEY, found {}; export it with `microtun-signer public-key`",
                    pem.tag()
                );
            }
            pem.contents().to_vec()
        } else {
            data
        };
        let raw: [u8; 65] = match der.len() {
            65 if der[0] == 0x04 => der.try_into().unwrap(),
            91 if der.starts_with(&SPKI_PREFIX) && der[26] == 0x04 => der[26..].try_into().unwrap(),
            n => panic!("{path}: not an ECDSA P-256 public key ({n} bytes)"),
        };
        keys.push(raw);
    }
    if keys.is_empty() {
        panic!("MICROTUN_BOOT_PUBKEYS lists no keys");
    }

    let mut out = format!("pub const PUBLIC_KEYS: [[u8; 65]; {}] = [\n", keys.len());
    for k in &keys {
        out += &format!("    {k:?},\n");
    }
    out += "];\n";
    write_out("keys.rs", out);
}

/// The `.pub.pem` file of the local key the bootloader trusts by default
/// (`microtun-signer local path --key-id <id>`, which also refreshes it).
fn local_public_key_file() -> String {
    let url = env::var("MICROTUN_SIGNER_URL")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| LOCAL_SIGNER_URL.into());
    if !is_local_signer_url(&url) {
        panic!(
            "MICROTUN_BOOT_PUBKEYS is unset but MICROTUN_SIGNER_URL={url} is a signer service; \
             set MICROTUN_BOOT_PUBKEYS to that firmware key's pinned public key (see SIGNING.md). \
             Builds only take a default key from microtun-signer's local store."
        );
    }
    let key_id = [FIRMWARE_KEY_ID_ENV, "MICROTUN_SIGNER_KEY_ID"]
        .into_iter()
        .find_map(|var| env::var(var).ok().filter(|id| !id.is_empty()))
        .unwrap_or_else(|| FIRMWARE_DEV_KEY_ID.into());
    let program = env::var("MICROTUN_SIGNER").unwrap_or_else(|_| "microtun-signer".into());
    let output = std::process::Command::new(&program)
        .args(["--url", &url, "local", "path", "--key-id", &key_id])
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "cannot run {program}: {e}; install microtun-signer or set MICROTUN_BOOT_PUBKEYS"
            )
        });
    if !output.status.success() {
        panic!(
            "`{program} --url {url} local path --key-id {key_id}` failed ({}):\n{}\n\
             For a new key, run `microtun-signer local init --key-id {key_id} --algorithm p256`, or set MICROTUN_BOOT_PUBKEYS.",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim_end()
        );
    }
    let path = String::from_utf8(output.stdout)
        .unwrap_or_else(|_| panic!("{program} printed a non-UTF-8 path"));
    path.trim_end_matches(['\r', '\n']).to_string()
}

/// Configure a microtun boot ESP *application* (or the third-stage bootloader)
/// using `esp-hal`: link `esp-hal`'s `linkall.x`, then the `microtun-boot.x`
/// contract exported by `microtun-boot-esp` (boot-info reservation). The order
/// matters: the contract checks memory regions defined by `linkall.x`.
///
/// No board input is needed: ESP slots come from the partition table at
/// runtime, and the chip is fixed by `microtun-boot-esp`'s chip feature, which
/// also selects `esp-hal`'s (a conflicting `esp-hal` chip fails to build).
///
/// Link arguments only apply to the package that emits them, so this MUST run
/// in the final application's `build.rs`. Requires `microtun-boot-esp` as a
/// direct dependency (checked through its `links` metadata) and
/// `microtun-boot-build` as a build dependency.
///
/// ```ignore
/// // application build.rs
/// fn main() {
///     microtun_boot_build::configure_esp_application();
/// }
/// ```
///
/// Only the bootloader verifies signatures, so only its `build.rs` also calls
/// [`emit_public_keys`]; applications embed no keys.
pub fn configure_esp_application() {
    platform_chip("microtun-boot-esp");
    println!("cargo:rustc-link-arg=-Tlinkall.x");
    println!("cargo:rustc-link-arg=-Tmicrotun-boot.x");
}

/// Configure a microtun boot RP2350/RP2354 *application* using `cortex-m-rt`:
/// write the `memory.x` that `cortex-m-rt`'s `link.x` includes and link
/// `link.x`.
///
/// `memory.x` holds the MEMORY regions: the application's execution window
/// (`chips/rp235x`), limited to the board's `max_image_size` (the slot size
/// less the MCUboot trailer), and SRAM below the boot-info reservation. It then
/// includes `microtun-boot-rp.x`, the chip's application contract exported by
/// `microtun-boot-rp`.
///
/// Link arguments only apply to the package that emits them, so this MUST run
/// in the final application's `build.rs`. Requires `microtun-boot-rp` as a direct
/// dependency, `cortex-m-rt`, and the `embassy-rp/imagedef-none` feature. The
/// application must not provide its own `memory.x`.
///
/// ```ignore
/// // application build.rs
/// fn main() {
///     microtun_boot_build::configure_rp2350_application(
///         microtun_boot_layout_rp2350_pico2::MAX_IMAGE_SIZE,
///     );
/// }
/// ```
///
/// Applications embed no keys: [`emit_public_keys`] is for the bootloader's
/// `build.rs` only.
pub fn configure_rp2350_application(max_image_size: u32) {
    use microtun_boot_chip_rp235x::{
        APP_XIP_BASE, APP_XIP_WINDOW_SIZE, BOOT_INFO_ADDRESS, BOOT_INFO_SIZE, CHIP, HEADER_SIZE,
        RAM_BASE,
    };

    check_platform_chip("microtun-boot-rp", CHIP);
    assert!(
        max_image_size > HEADER_SIZE && max_image_size <= APP_XIP_WINDOW_SIZE,
        "max_image_size {max_image_size:#x} must exceed the header and fit the XIP window"
    );
    let memory = format!(
        "/* @generated by microtun_boot_build::configure_rp2350_application. */
MEMORY
{{
    FLASH     : ORIGIN = {:#010x}, LENGTH = {:#x}
    RAM       : ORIGIN = {RAM_BASE:#010x}, LENGTH = {:#x}
    BOOT_INFO : ORIGIN = {BOOT_INFO_ADDRESS:#010x}, LENGTH = {BOOT_INFO_SIZE:#x}
}}

INCLUDE microtun-boot-rp.x
",
        APP_XIP_BASE + HEADER_SIZE,
        max_image_size - HEADER_SIZE,
        BOOT_INFO_ADDRESS - RAM_BASE,
    );
    write_out("memory.x", memory);
    println!("cargo:rustc-link-search={}", out_dir().display());
    println!("cargo:rustc-link-arg=-Tlink.x");
}
