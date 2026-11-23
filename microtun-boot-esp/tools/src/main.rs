//! `microtun-boot-tools-esp`: ESP32 images, Secure Boot V2 and the IDF boot chain.
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use microtun_boot_build::tools::{
    self as tools,
    cli::{self, SignerArgs, Signing},
    mcuboot::{self, SignOptions},
};
use microtun_boot_tools_esp::{DEV_KEY, chain, flash, geometry, image, layout, sbv2};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    about = "microtun boot ESP32 image, Secure Boot V2 and boot-chain tools",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Convert and MCUboot-sign an application.
    Sign {
        elf: PathBuf,
        output: PathBuf,
        #[command(flatten)]
        signing: Signing,
    },
    /// MCUboot-sign a raw payload with this board's geometry.
    SignRaw(cli::SignRaw),
    /// Check signed envelope, ESP segments and MMU mapping.
    Inspect {
        image: PathBuf,
        #[arg(value_parser = tools::number)]
        slot: Option<u32>,
    },
    /// RSA-3072/PSS/SHA256 ROM signing of a padded ESP image.
    SignSbv2 {
        image: PathBuf,
        output: PathBuf,
        /// The key must be RSA-3072 (local default: `microtun-boot-local-esp32c3-sbv2-rsa3072`).
        #[command(flatten)]
        signer: SignerArgs,
    },
    /// Verify against a trusted RSA-3072 public key.
    VerifySbv2 {
        image: PathBuf,
        /// Public key PEM [default: the configured signer's public key].
        #[arg(long)]
        key: Option<PathBuf>,
        #[command(flatten)]
        signer: SignerArgs,
    },
    /// Build or check the IDF second stage.
    BuildBootloader { output: PathBuf },
    /// Erase an entire slot, then sign, flash and monitor only the image bytes,
    /// or save the image with NO_FLASH=1.
    FlashApp {
        elf: PathBuf,
        /// Exit after the reset instead of opening the serial monitor.
        #[arg(long)]
        no_monitor: bool,
    },
    /// Sign a bootloader ELF and flash it, or use --signed-bin for an existing signed image.
    FlashBootloader {
        /// Bootloader ELF by default; signed boot-chain binary with --signed-bin.
        image: PathBuf,
        /// Verify and flash an existing signed merged boot chain without signing.
        /// Required for binary input, regardless of filename extension.
        #[arg(long)]
        signed_bin: bool,
        /// The key must be RSA-3072 (local default: `microtun-boot-local-esp32c3-sbv2-rsa3072`).
        #[command(flatten)]
        signer: SignerArgs,
    },
    /// Validate partitions; print constants or emit the binary table.
    Layout {
        #[arg(long)]
        partitions: Option<PathBuf>,
        #[arg(long)]
        table: Option<PathBuf>,
        #[arg(long)]
        get: Option<String>,
        #[arg(long)]
        constant: Option<String>,
    },
}

fn main() {
    cli::exit_on_error(env!("CARGO_BIN_NAME"), run(Cli::parse().command));
}
fn run(command: Command) -> Result<()> {
    match command {
        Command::Sign {
            elf,
            output,
            signing,
        } => {
            let geometry = geometry()?;
            let options = signing.options(geometry)?;
            tools::distinct_output(&output, &[&elf])?;
            let data = mcuboot::sign(&image::convert(&elf, false)?, geometry, &options)?;
            image::inspect_slot(
                &data,
                layout::default_layout()?.region("image-0")?.0,
                geometry,
            )?;
            tools::write_output(&output, data)
        }
        Command::SignRaw(args) => args.run(geometry()?),
        Command::Inspect { image: path, slot } => {
            image::inspect_slot(
                &fs::read(path)?,
                slot.unwrap_or(layout::default_layout()?.region("image-0")?.0),
                geometry()?,
            )?;
            println!("OK");
            Ok(())
        }
        Command::SignSbv2 {
            image,
            output,
            signer,
        } => {
            let signer = signer.signer(DEV_KEY)?;
            tools::distinct_output(&output, &[&image])?;
            tools::write_output(&output, sbv2::sign(&fs::read(image)?, &signer)?)
        }
        Command::VerifySbv2 { image, key, signer } => {
            let pem = match key {
                Some(path) => fs::read_to_string(path)?,
                None => signer.signer(DEV_KEY)?.public_key()?,
            };
            sbv2::verify(&fs::read(image)?, &pem)
        }
        Command::BuildBootloader { output } => {
            let settings = chain::Settings::from_env();
            if let Some(source) = &settings.idf_bootloader {
                tools::distinct_output(&output, &[source])?;
            }
            tools::write_output(&output, chain::build_idf(&settings)?)
        }
        Command::FlashApp { elf, no_monitor } => {
            let layout = layout::default_layout()?;
            let slot = tools::env("SLOT", "image-0");
            ensure!(
                ["image-0", "image-1"].contains(&slot.as_str()),
                "SLOT must be image-0 or image-1"
            );
            let geometry = geometry()?;
            let options = SignOptions::from_env(geometry)?;
            let data = mcuboot::sign(&image::convert(&elf, false)?, geometry, &options)?;
            let (offset, size) = layout.region(&slot)?;
            image::inspect_slot(&data, offset, geometry)?;
            ensure!(data.len() <= size as usize, "signed image exceeds slot");
            flash_or_save_app(
                &elf,
                &data,
                offset,
                size,
                tools::flag("NO_FLASH"),
                !no_monitor,
                &[&elf],
            )
        }
        Command::FlashBootloader {
            image: elf,
            signed_bin,
            signer,
        } => {
            if signed_bin {
                return chain::flash_signed_merged(&elf, tools::flag("NO_FLASH"));
            }
            let signer = signer.signer(DEV_KEY)?;
            let settings = chain::Settings::from_env();
            let chain = chain::boot_chain(&elf, &settings, &signer)?;
            let parent = elf
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .context("ELF path must include a directory")?;
            for name in chain::OUTPUTS {
                tools::distinct_output(&parent.join(name), &[&elf])?;
            }
            chain.write(parent)?;
            if settings.no_flash {
                Ok(())
            } else {
                chain.flash(parent)
            }
        }
        Command::Layout {
            partitions,
            table,
            get,
            constant,
        } => {
            let layout = if let Some(path) = partitions {
                layout::EspLayout::load(&path)?
            } else {
                layout::default_layout()?
            };
            if let Some(output) = table {
                tools::write_output(&output, layout.binary()?)?;
            }
            if let Some(name) = get {
                let (offset, size) = layout.region(&name)?;
                println!("{offset:#x} {size:#x}");
                return Ok(());
            }
            cli::print_constants(&layout.constants()?, constant.as_deref())
        }
    }
}

/// `NO_FLASH=1` retains the signed image alongside its ELF instead of touching
/// the device. Otherwise erase the entire slot, including its old MCUboot
/// trailer, then flash just these signed bytes and, unless `--no-monitor`,
/// monitor.
fn flash_or_save_app(
    elf: &Path,
    signed: &[u8],
    offset: u32,
    size: u32,
    no_flash: bool,
    monitor: bool,
    inputs: &[&Path],
) -> Result<()> {
    if no_flash {
        let output = elf.with_extension("signed.bin");
        tools::distinct_output(&output, inputs)?;
        tools::write_output(&output, signed)?;
        println!("NO_FLASH=1: wrote {}", output.display());
        return Ok(());
    }
    flash::replace_region(offset, size, signed, monitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_bootloader_parses() {
        let args = Cli::try_parse_from(["esp-tools", "flash-bootloader", "boot.elf"]).unwrap();
        assert!(matches!(args.command, Command::FlashBootloader { .. }));
    }

    #[test]
    fn flash_app_monitors_unless_disabled() {
        let parse = |args: &[&str]| match Cli::try_parse_from(args).unwrap().command {
            Command::FlashApp { no_monitor, .. } => no_monitor,
            _ => panic!("expected flash-app"),
        };
        assert!(!parse(&["esp-tools", "flash-app", "fw.elf"]));
        assert!(parse(&["esp-tools", "flash-app", "fw.elf", "--no-monitor"]));
        // The boot chain is never monitored, so it has no such flag.
        assert!(
            Cli::try_parse_from(["esp-tools", "flash-bootloader", "boot.elf", "--no-monitor"])
                .is_err()
        );
    }

    #[test]
    fn signed_bootloader_requires_explicit_flag_even_with_bin_extension() {
        let args = Cli::try_parse_from([
            "esp-tools",
            "flash-bootloader",
            "esp32c3-supermini-release-boot-chain.sbv2.bin",
        ])
        .unwrap();
        assert!(matches!(
            args.command,
            Command::FlashBootloader {
                signed_bin: false,
                ..
            }
        ));

        for name in ["boot-chain.sbv2.bin", "boot-chain", "boot-chain.elf"] {
            let args = Cli::try_parse_from(["esp-tools", "flash-bootloader", "--signed-bin", name])
                .unwrap();
            assert!(matches!(
                args.command,
                Command::FlashBootloader {
                    signed_bin: true,
                    ..
                }
            ));
        }
    }

    #[test]
    fn no_flash_saves_signed_image_without_flashing() {
        let dir = tempfile::tempdir().unwrap();
        let elf = dir.path().join("esp32c3-supermini-firmware");
        fs::write(&elf, b"ELF").unwrap();

        // No serial port or connected ESP32 is needed for this path.
        flash_or_save_app(&elf, b"signed", 0x6_f000, 0x1000, true, true, &[&elf]).unwrap();
        assert_eq!(
            fs::read(elf.with_extension("signed.bin")).unwrap(),
            b"signed"
        );
        assert_eq!(fs::read(&elf).unwrap(), b"ELF");
    }

    #[test]
    fn no_flash_does_not_overwrite_an_input() {
        let dir = tempfile::tempdir().unwrap();
        let elf = dir.path().join("esp32c3-supermini-firmware");
        let input = elf.with_extension("signed.bin");
        fs::write(&input, b"input").unwrap();

        assert!(
            flash_or_save_app(
                &elf,
                b"signed",
                0x6_f000,
                0x1000,
                true,
                true,
                &[&elf, &input]
            )
            .is_err()
        );
        assert_eq!(fs::read(input).unwrap(), b"input");
    }
}
