//! `microtun-boot-tools-rp`: checked RP2350 ELF conversion, signing, ROM
//! sealing, SWD loading with a serial monitor and the fixture flash algorithm.
use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use microtun_boot_build::tools::{
    self as tools,
    cli::{self, SignerArgs, Signing},
    mcuboot::{self, SignOptions},
};
use microtun_boot_layout_rp2350_pico2 as layout;
use microtun_boot_tools_rp::{
    DEV_KEY, algorithm, check_signed_app, constants, download, elf, flash_app, geometry, seal,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command as Process,
};

#[derive(Parser)]
#[command(
    about = "microtun boot RP2350 image, ROM sealing and SWD tools",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Validate ELF, vectors, SRAM reservation and ROM load map.
    Check {
        elf: PathBuf,
        #[arg(long)]
        bootloader: bool,
    },
    /// Export checked physical load bytes.
    ExportBin {
        elf: PathBuf,
        output: PathBuf,
        #[arg(long)]
        bootloader: bool,
    },
    /// MCUboot-sign an application.
    Sign {
        elf: PathBuf,
        output: PathBuf,
        #[command(flatten)]
        signing: Signing,
    },
    /// MCUboot-sign a raw payload with this board's geometry.
    SignRaw(cli::SignRaw),
    /// ROM-seal the bootloader and independently check metadata/key.
    SignBootloader {
        elf: PathBuf,
        output: PathBuf,
        #[arg(long)]
        otp_output: Option<PathBuf>,
        /// The key must be secp256k1 (local default: `microtun-boot-local-rp2350-rom-secp256k1`).
        #[command(flatten)]
        signer: SignerArgs,
    },
    /// Erase the whole slot, download an unpadded signed app over SWD, reset
    /// and monitor UART0, or save the image with NO_FLASH=1.
    FlashApp {
        elf: PathBuf,
        #[arg(long, value_enum, default_value = "primary")]
        slot: Slot,
        /// Exit after the reset instead of monitoring UART0 (115200 8N1).
        #[arg(long)]
        no_monitor: bool,
    },
    /// ROM-seal a bootloader ELF and flash it, or use --signed-bin for a signed image.
    FlashBootloader {
        /// Bootloader ELF by default; ROM-sealed binary with --signed-bin.
        image: PathBuf,
        /// Verify and flash an existing ROM-sealed bootloader without signing.
        /// Required for binary input, regardless of filename extension.
        #[arg(long)]
        signed_bin: bool,
        /// The key must be secp256k1 (local default: `microtun-boot-local-rp2350-rom-secp256k1`).
        #[command(flatten)]
        signer: SignerArgs,
    },
    /// Build/package the fixture flash algorithm as probe-rs YAML.
    FlashAlgorithm {
        #[arg(long)]
        elf: Option<PathBuf>,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, env = "CARGO", default_value = "cargo")]
        cargo: String,
    },
    /// Print the compiled board layout (edit layout/partitions.json and rebuild).
    Layout {
        #[arg(long)]
        constant: Option<String>,
    },
}
#[derive(Clone, Copy, ValueEnum)]
enum Slot {
    Primary,
    Secondary,
}

fn main() {
    cli::exit_on_error(env!("CARGO_BIN_NAME"), run(Cli::parse().command));
}
fn run(command: Command) -> Result<()> {
    match command {
        Command::Check {
            elf: path,
            bootloader,
        } => {
            let image = elf::image(&fs::read(path)?, bootloader)?;
            println!(
                "checked origin {:#x}, {:#x} payload bytes",
                image.origin,
                image.payload.len()
            );
            Ok(())
        }
        Command::ExportBin {
            elf: path,
            output,
            bootloader,
        } => {
            tools::distinct_output(&output, &[&path])?;
            tools::write_output(&output, elf::image(&fs::read(path)?, bootloader)?.payload)
        }
        Command::Sign {
            elf: path,
            output,
            signing,
        } => {
            let geometry = geometry();
            let options = signing.options(geometry)?;
            tools::distinct_output(&output, &[&path])?;
            let image = elf::image(&fs::read(path)?, false)?;
            let signed = mcuboot::sign(&image.payload, geometry, &options)?;
            check_signed_app(&signed)?;
            tools::write_output(&output, signed)
        }
        Command::SignRaw(args) => args.run(geometry()),
        Command::SignBootloader {
            elf: path,
            output,
            otp_output,
            signer,
        } => {
            let signer = signer.signer(DEV_KEY)?;
            tools::distinct_output(&output, &[&path])?;
            if let Some(otp) = &otp_output {
                tools::distinct_output(otp, &[&path, &output])?;
            }
            let image = elf::image(&fs::read(path)?, true)?;
            let sealed = seal::seal(&image, &signer, otp_output.as_deref())?;
            tools::write_output(&output, sealed)
        }
        Command::FlashApp {
            elf: path,
            slot,
            no_monitor,
        } => {
            let image = elf::image(&fs::read(&path)?, false)?;
            let geometry = geometry();
            let options = SignOptions::from_env(geometry)?;
            let signed = mcuboot::sign(&image.payload, geometry, &options)?;
            check_signed_app(&signed)?;
            let offset = match slot {
                Slot::Primary => layout::PRIMARY_OFFSET,
                Slot::Secondary => layout::SECONDARY_OFFSET,
            };
            flash_or_save_app(
                &path,
                &signed,
                layout::XIP_BASE + offset,
                tools::flag("NO_FLASH"),
                !no_monitor,
                &[&path],
            )
        }
        Command::FlashBootloader {
            image: path,
            signed_bin,
            signer,
        } => {
            let no_flash = tools::flag("NO_FLASH");
            if signed_bin {
                let data = fs::read(&path)?;
                seal::verify_existing(&data)?;
                if no_flash {
                    println!("NO_FLASH=1: verified existing signed bootloader; skipped flashing");
                    return Ok(());
                }
                tools::require_provisioning_ack()?;
                // Use the already verified bytes rather than re-reading a mutable source.
                return write_or_load(&path, &data, layout::XIP_BASE, false, &[&path]);
            }
            let signer = signer.signer(DEV_KEY)?;
            if !no_flash {
                tools::require_provisioning_ack()?;
            }
            let image = elf::image(&fs::read(&path)?, true)?;
            let sealed = seal::seal(&image, &signer, None)?;
            write_or_load(&path, &sealed, image.origin, no_flash, &[&path])
        }
        Command::FlashAlgorithm {
            elf: path,
            output,
            cargo,
        } => flash_algorithm(path, output, cargo),
        Command::Layout { constant } => cli::print_constants(&constants(), constant.as_deref()),
    }
}

fn flash_or_save_app(
    elf: &Path,
    signed: &[u8],
    address: u32,
    no_flash: bool,
    monitor: bool,
    inputs: &[&Path],
) -> Result<()> {
    if no_flash {
        return write_or_load(elf, signed, address, true, inputs);
    }
    // Erase the selected slot, including its old MCUboot trailer, before
    // programming just these signed bytes through the same SWD session.
    flash_app(signed, address, monitor)
}

fn flash_algorithm(elf: Option<PathBuf>, output: Option<PathBuf>, cargo: String) -> Result<()> {
    let dir = tools::repo_root().join("boards/rp2350-pico2/flash-algorithm");
    let elf = if let Some(elf) = elf {
        elf
    } else {
        // Use an explicit target and no inherited board linker flags.
        tools::progress::run(
            Process::new(cargo)
                .current_dir(&dir)
                .env_remove("RUSTFLAGS")
                .env_remove("CARGO_ENCODED_RUSTFLAGS")
                .args([
                    "build",
                    "--release",
                    "--bin",
                    "flash",
                    "--target",
                    algorithm::TARGET,
                ]),
            "Building RP2350 flash algorithm",
        )?;
        dir.join("target")
            .join(algorithm::TARGET)
            .join("release/flash")
    };
    let output = output.unwrap_or_else(|| dir.join("target.yaml"));
    tools::distinct_output(&output, &[&elf])?;
    tools::write_output(&output, algorithm::generate(&fs::read(elf)?)?)
}

/// `NO_FLASH=1` retains the signed image alongside its ELF instead of touching
/// the device. Ordinary runners still use a temporary file for SWD flashing.
fn write_or_load(
    elf: &Path,
    signed: &[u8],
    address: u32,
    no_flash: bool,
    inputs: &[&Path],
) -> Result<()> {
    if no_flash {
        let output = elf.with_extension("signed.bin");
        tools::distinct_output(&output, inputs)?;
        tools::write_output(&output, signed)?;
        println!("NO_FLASH=1: wrote {}", output.display());
        Ok(())
    } else {
        let dir = tempfile::tempdir()?;
        let bin = dir.path().join("image.bin");
        fs::write(&bin, signed)?;
        download(&bin, address)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flash_bootloader_parses() {
        let args = Cli::try_parse_from(["rp-tools", "flash-bootloader", "boot.elf"]).unwrap();
        assert!(matches!(args.command, Command::FlashBootloader { .. }));
    }

    #[test]
    fn signed_bootloader_requires_explicit_flag_even_with_bin_extension() {
        let args = Cli::try_parse_from([
            "rp-tools",
            "flash-bootloader",
            "rp2350-pico2-release-microtun-boot.signed.bin",
        ])
        .unwrap();
        assert!(matches!(
            args.command,
            Command::FlashBootloader {
                signed_bin: false,
                ..
            }
        ));

        for name in [
            "rp2350-pico2-microtun-boot.signed.bin",
            "bootloader",
            "bootloader.elf",
        ] {
            let args = Cli::try_parse_from(["rp-tools", "flash-bootloader", "--signed-bin", name])
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
    fn flash_app_is_the_application_command() {
        let args = Cli::try_parse_from(["rp-tools", "flash-app", "firmware.elf"]).unwrap();
        assert!(matches!(args.command, Command::FlashApp { .. }));
    }

    #[test]
    fn flash_app_monitors_unless_disabled() {
        let parse = |args: &[&str]| match Cli::try_parse_from(args).unwrap().command {
            Command::FlashApp { no_monitor, .. } => no_monitor,
            _ => panic!("expected flash-app"),
        };
        assert!(!parse(&["rp-tools", "flash-app", "fw.elf"]));
        assert!(parse(&["rp-tools", "flash-app", "fw.elf", "--no-monitor"]));
        assert!(parse(&[
            "rp-tools",
            "flash-app",
            "fw.elf",
            "--slot",
            "secondary",
            "--no-monitor"
        ]));
        // As in microtun-boot-tools-esp, the bootloader never monitors.
        assert!(
            Cli::try_parse_from(["rp-tools", "flash-bootloader", "boot.elf", "--no-monitor"])
                .is_err()
        );
    }

    #[test]
    fn no_flash_saves_signed_image_without_loading() {
        let dir = tempfile::tempdir().unwrap();
        let elf = dir.path().join("rp2350-pico2-microtun-boot");
        fs::write(&elf, b"ELF").unwrap();

        // No SWD probe or connected RP2350 is needed for this path.
        flash_or_save_app(&elf, b"sealed", 0x1002_0000, true, true, &[&elf]).unwrap();
        assert_eq!(
            fs::read(elf.with_extension("signed.bin")).unwrap(),
            b"sealed"
        );
        assert_eq!(fs::read(&elf).unwrap(), b"ELF");
    }

    #[test]
    fn no_flash_does_not_overwrite_an_input() {
        let dir = tempfile::tempdir().unwrap();
        let elf = dir.path().join("rp2350-pico2-microtun-boot");
        let input = elf.with_extension("signed.bin");
        fs::write(&input, b"input").unwrap();

        assert!(write_or_load(&elf, b"sealed", 0x1000_0000, true, &[&elf, &input]).is_err());
        assert_eq!(fs::read(input).unwrap(), b"input");
    }

    #[test]
    fn rom_commands_take_signer_options() {
        let args = Cli::try_parse_from([
            "rp-tools",
            "sign-bootloader",
            "boot.elf",
            "boot.bin",
            "--signer-url",
            "local:/tmp/keys",
            "--signer-key-id",
            "board-rom",
        ])
        .unwrap();
        let Command::SignBootloader { signer, .. } = args.command else {
            panic!("expected sign-bootloader");
        };
        let signer = signer.signer(DEV_KEY).unwrap();
        assert_eq!(
            (signer.url.as_str(), signer.key_id.as_str()),
            ("local:/tmp/keys", "board-rom")
        );
    }
}
