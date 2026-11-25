//! The board's whole flash: the signed ESP-IDF second stage at the chip's
//! ROM offset (`0x0` on the ESP32-C3 and ESP32-C6, `0x2000` on the ESP32-C5), the
//! partition table, signed factory microtun boot, then the data slots.
//! Everything is derived from `boards/<board>/layout/partitions.csv` by
//! `microtun-boot-tools-esp layout`, as `microtun-boot-tools-esp
//! flash-bootloader` does. The composer itself is [`crate::flash`].
//!
//! The fixture always covers the complete flash, but the worker writes it
//! from the ROM second-stage offset on, like the board's flashing tool: on
//! the ESP32-C5 the sectors below `0x2000` belong to the ROM's key manager
//! and are left untouched.

use std::{path::Path, process::Command, time::Duration};

use microtun_boot_core::{Policy, SlotLayout};

pub use crate::flash::{FlashImage, Request, SECTOR, trusted_keys};

use super::EspTarget;
use crate::flash::describe_size;

/// The same slot layout the target discovers in the flashed table.
pub fn slot_layout(target: &'static EspTarget) -> crate::Result<SlotLayout> {
    Ok(super::partitions::read(target)?.slots)
}

/// Erased flash with both signed boot stages and the table used for signing.
/// The ROM verifies `idf_image`; ESP-IDF verifies `image` in its factory slot.
pub fn compose(
    target: &'static EspTarget,
    chain: &super::artifacts::Bootloader,
) -> crate::Result<FlashImage> {
    let chip = target.chip();
    let table = super::partitions::read(target)?;
    let stage2 = chip.rom_bootloader_offset;
    // The signed second stage ends before the partition table.
    let room = (chip.max_stage2_signed as usize).min((table.offset - stage2) as usize);
    if chain.idf_image.len() > room {
        return Err(format!(
            "signed IDF second stage exceeds {room:#x} bytes at {stage2:#x} \
             (signed-stage limit or table offset)"
        )
        .into());
    }
    if chain.image.len() > table.factory.size as usize {
        return Err("signed microtun boot exceeds its factory partition".into());
    }
    let flash = FlashImage::erased(
        target.board(),
        table.slots,
        Some(table.storage),
        Policy::default(),
    );
    flash
        .program(stage2, &chain.idf_image)
        .program(table.offset, &table.bytes)
        .program(table.factory.offset, &chain.image);
    Ok(flash)
}

fn validate_image_size(target: &EspTarget, image: &Path) -> crate::Result<()> {
    let expected = target.tools.flash_size as u64;
    let size = std::fs::metadata(image)?.len();
    if size != expected {
        return Err(format!(
            "expected complete {} flash image, got {size} bytes",
            describe_size(expected as usize)
        )
        .into());
    }
    Ok(())
}

/// The bytes the worker writes, and where: everything from the chip's ROM
/// second-stage offset to the end of flash.
fn written_region<'a>(target: &EspTarget, data: &'a [u8]) -> (u32, &'a [u8]) {
    let base = target.chip().rom_bootloader_offset;
    (base, &data[base as usize..])
}

/// Flash the fixture through the exact ROM command implementation used by
/// the ESP host tools (SPI_ATTACH, SPI_SET_PARAMS, FLASH_BEGIN, FLASH_DATA).
/// No RAM stub, FLASH_END, or reset is used. The parent opens CDC for boot
/// capture and performs its existing USB-Serial/JTAG reset afterward.
pub fn write_full_image_in_worker(
    target: &EspTarget,
    port: &str,
    image: &Path,
) -> crate::Result<()> {
    validate_image_size(target, image)?;
    let data = std::fs::read(image)?;
    let (address, bytes) = written_region(target, &data);
    eprintln!(
        "[fixture] {}: writing {} from {address:#x} via shared ROM commands (no stub, no reset)",
        target.chip().display,
        describe_size(bytes.len()),
    );
    microtun_boot_tools_esp::flash::write_rom_region_no_reset(target.tools, port, address, bytes)
        .map_err(|error| crate::Error::external(format!("{error:#}")))
}

/// The shared `microtun-boot-flash` worker isolates USB I/O so a hung operation
/// can be killed before the next HIL test takes the board lock.
fn worker_command(target: &EspTarget, executable: &Path, port: &str, image: &Path) -> Command {
    let mut command = Command::new(executable);
    command.arg(target.tools.name).arg(port).arg(image);
    command
}

/// HIL fixture flashing, supervised to ensure USB errors cannot hang the test
/// indefinitely. No external Python executable, pre-read or differential scan.
/// The caller reopens CDC after the worker exits and resets into the image.
pub fn write_full_image(target: &'static EspTarget, port: &str, image: &Path) -> crate::Result<()> {
    validate_image_size(target, image)?;
    let board = target.board();
    let var = board.env("FLASH_TIMEOUT_SECS");
    let seconds = match std::env::var(&var) {
        Ok(value) => value
            .parse::<u64>()
            .map_err(|_| format!("{var} must be a positive integer"))?,
        Err(std::env::VarError::NotPresent) => 600,
        Err(error) => return Err(error.into()),
    };
    if seconds == 0 {
        return Err(format!("{var} must be positive").into());
    }
    let mut command = worker_command(target, &crate::dut::flash_worker(board)?, port, image);
    crate::process::run(
        &mut command,
        "ESP ROM fixture write",
        Duration::from_secs(seconds),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esp::targets;
    use std::ffi::OsString;

    #[test]
    fn fixture_invokes_bounded_rust_worker_with_full_image_and_port() {
        for target in targets() {
            let image = Path::new("/tmp/flash fixture.bin");
            let command = worker_command(
                target,
                Path::new("/tmp/microtun-boot-flash"),
                "/dev/serial/by-id/esp-board",
                image,
            );
            assert_eq!(command.get_program(), "/tmp/microtun-boot-flash");
            let args: Vec<OsString> = command.get_args().map(OsString::from).collect();
            assert_eq!(
                args,
                [
                    target.tools.name,
                    "/dev/serial/by-id/esp-board",
                    "/tmp/flash fixture.bin"
                ]
                .map(OsString::from)
            );
        }
    }

    #[test]
    fn rejects_incomplete_fixture_before_flashing() {
        let image = tempfile::NamedTempFile::new().unwrap();
        for target in targets() {
            let size = describe_size(target.tools.flash_size as usize);
            for error in [
                write_full_image(target, "unused", image.path()).unwrap_err(),
                write_full_image_in_worker(target, "unused", image.path()).unwrap_err(),
            ] {
                assert!(
                    error
                        .to_string()
                        .contains(&format!("expected complete {size}")),
                    "{error}"
                );
            }
        }
    }

    /// The C5 fixture is exactly the board's 4 MiB, and an image of another
    /// size is refused before any USB traffic.
    #[cfg(feature = "esp32c5-mini")]
    #[test]
    fn c5_fixture_matches_the_board_and_rejects_other_sizes() {
        const MIB: u64 = 1 << 20;
        for target in targets().filter(|target| target.chip().name == "esp32c5") {
            let size = u64::from(target.tools.flash_size);
            assert_eq!(size, 4 * MIB);
            let chain = super::super::artifacts::Bootloader {
                image: vec![0xa5; 64],
                idf_image: vec![0x5a; 64],
            };
            let flash = compose(target, &chain).unwrap();
            let bytes = flash.bytes();
            assert_eq!(bytes.len() as u64, size);
            let (address, written) = written_region(target, &bytes);
            assert_eq!(address, 0x2000);
            assert_eq!(written.len() as u64, size - 0x2000);

            let image = tempfile::NamedTempFile::new().unwrap();
            image.as_file().set_len(size).unwrap();
            validate_image_size(target, image.path()).unwrap();
            let expected = format!("expected complete {} MiB", size / MIB);
            for other in [4 * MIB, 8 * MIB, 16 * MIB] {
                if other == size {
                    continue;
                }
                image.as_file().set_len(other).unwrap();
                for error in [
                    write_full_image(target, "unused", image.path()).unwrap_err(),
                    write_full_image_in_worker(target, "unused", image.path()).unwrap_err(),
                ] {
                    assert!(error.to_string().contains(&expected), "{error}");
                }
            }
        }
    }

    /// The second stage lands at the ROM's offset and nothing is written
    /// below it, so the ESP32-C5 key-manager sectors stay untouched.
    #[test]
    fn second_stage_is_placed_and_written_from_the_rom_offset() {
        for target in targets() {
            let chain = super::super::artifacts::Bootloader {
                image: vec![0xa5; 64],
                idf_image: vec![0x5a; 64],
            };
            let flash = compose(target, &chain).unwrap();
            let bytes = flash.bytes();
            let base = target.chip().rom_bootloader_offset as usize;
            assert!(bytes[..base].iter().all(|byte| *byte == 0xff));
            assert_eq!(&bytes[base..base + 64], &chain.idf_image[..]);
            let (address, written) = written_region(target, &bytes);
            assert_eq!(address as usize, base);
            assert_eq!(written.len(), bytes.len() - base);
            assert_eq!(written[0], 0x5a);
        }
    }

    #[test]
    fn rejects_a_second_stage_that_reaches_the_partition_table() {
        for target in targets() {
            let table = super::super::partitions::read(target).unwrap();
            let room = table.offset - target.chip().rom_bootloader_offset;
            let chain = super::super::artifacts::Bootloader {
                image: vec![0xa5; 64],
                idf_image: vec![0x5a; room as usize + 1],
            };
            assert!(compose(target, &chain).is_err());
        }
    }
}
