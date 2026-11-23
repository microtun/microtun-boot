//! Clap pieces shared by the board tool binaries.
use super::{
    mcuboot::{self, Geometry, SignOptions},
    signer::{DevKey, FIRMWARE_DEV_KEY, Signer},
};
use anyhow::{Context, Result};
use clap::Args;
use std::{collections::BTreeMap, fs, path::PathBuf};

/// Which microtun-signer key signs: a local development key by default.
#[derive(Args)]
pub struct SignerArgs {
    /// microtun-signer URL: `local:` (default) or `local:/absolute/dir` for
    /// development keys on this machine, or a signer service.
    #[arg(long, env = "MICROTUN_SIGNER_URL")]
    pub signer_url: Option<String>,
    /// Signing key ID. Defaults to the purpose's variable (MICROTUN_BOOT_KEY_ID,
    /// ESP_SBV2_KEY_ID or RP2350_SECURE_BOOT_KEY_ID), then
    /// MICROTUN_SIGNER_KEY_ID, then, with a local: URL, the purpose's
    /// development key.
    #[arg(long)]
    pub signer_key_id: Option<String>,
    /// Authenticate to a signer service as a GitHub user (browser sign-in).
    #[arg(long, env = "MICROTUN_SIGNER_GITHUB_LOGIN")]
    pub signer_github_login: bool,
}
impl SignerArgs {
    /// The signer, with `default` as the key when the URL is local.
    pub fn signer(self, default: DevKey) -> Result<Signer> {
        Signer::new(
            self.signer_url,
            self.signer_key_id,
            self.signer_github_login,
            default,
        )
    }
}

/// MCUboot signing options.
#[derive(Args)]
pub struct Signing {
    /// The key must be P-256 (local default: `microtun-boot-local-p256`).
    #[command(flatten)]
    pub signer: SignerArgs,
    #[arg(long, env = "IMAGE_VERSION", default_value = "0.0.0")]
    pub version: String,
    #[arg(long)]
    pub pad: bool,
    #[arg(long)]
    pub confirm: bool,
    /// Extra imgtool arguments after -- take precedence over defaults.
    #[arg(last = true)]
    pub extra: Vec<String>,
}
impl Signing {
    pub fn options(self, geometry: Geometry) -> Result<SignOptions> {
        let mut options = SignOptions::new(geometry, self.signer.signer(FIRMWARE_DEV_KEY)?);
        options.version = self.version;
        if self.pad || self.confirm {
            options.extra.push("--pad".into());
        }
        if self.confirm {
            options.extra.push("--confirm".into());
        }
        options.extra.extend(self.extra);
        Ok(options)
    }
}

/// `sign-raw`: MCUboot-sign a raw payload with the board's geometry.
#[derive(Args)]
pub struct SignRaw {
    pub payload: PathBuf,
    pub output: PathBuf,
    #[command(flatten)]
    pub signing: Signing,
}
impl SignRaw {
    pub fn run(self, geometry: Geometry) -> Result<()> {
        let options = self.signing.options(geometry)?;
        super::distinct_output(&self.output, &[&self.payload])?;
        let signed = mcuboot::sign(&fs::read(&self.payload)?, geometry, &options)?;
        super::write_output(&self.output, signed)
    }
}

/// Print every layout constant as JSON, or one as hex.
pub fn print_constants(values: &BTreeMap<String, u32>, constant: Option<&str>) -> Result<()> {
    if let Some(name) = constant {
        println!("{:#x}", values.get(name).context("unknown constant")?);
    } else {
        println!("{}", serde_json::to_string_pretty(values)?);
    }
    Ok(())
}

/// Print `program: error` and exit with status 1 on failure.
pub fn exit_on_error(program: &str, result: Result<()>) {
    if let Err(error) = result {
        eprintln!("{program}: {error:#}");
        std::process::exit(1);
    }
}
