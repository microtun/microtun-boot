//! ESP32 support for the hardware scenarios, shared by every ESP board.
//!
//! The boards run the same three-stage chain: the mask ROM verifies the
//! signed ESP-IDF second stage, which verifies the signed microtun boot
//! factory app, which verifies the MCUboot-signed application. The suite
//! builds the real second stage, microtun boot and the example firmware,
//! converts them with espflash, signs the boot stages with the board's
//! Secure Boot V2 tooling, signs firmware with upstream `imgtool`, and
//! composes complete flash images with microtun-boot-core's own
//! updater/trailer code, including a test-hook record in the `storage`
//! partition that selects how each image version misbehaves. Each scenario
//! flashes the whole fixture using the Rust worker and the host tools' shared
//! ROM-only command writer over the chip's
//! native USB-Serial/JTAG, then opens USB CDC for boot capture and reset.
//! Later resets reuse the same CDC connection.
//!
//! One [`EspTarget`] describes each board (`crate::esp32c3::TARGET`,
//! `crate::esp32c5::TARGET`, `crate::esp32c6::TARGET`). Chip facts (ROM second-stage offset, flash
//! size, espflash chip and crystal, memory windows, Secure Boot key) come
//! from `microtun-boot-tools-esp`'s board table, the same values the board's
//! own flashing tool uses; the target adds only what the harness needs
//! beyond that. The modules below take the target as their first argument:
//! * [`artifacts`]: cached builds converted and SBv2-padded with espflash.
//! * [`esp_image`]: espflash conversion plus in-place mutations for malformed images.
//! * [`flash`]: partition table, flash image composition and shared ROM command flashing.
//! * [`sign`]: imgtool with the board's slot geometry.
//! * [`security_counter`]: the eFuse rollback floor, read over JTAG.
//!
//! Partition geometry is read from the exact binary table that the fixture
//! programs; nothing about the layout is compiled into the harness.

use std::{
    ops::Deref,
    sync::{LazyLock, OnceLock},
};

use crate::{Board, Console, Signing, artifacts::Variant};

pub mod artifacts;
pub mod esp_image;
pub mod flash;
mod partitions;
pub mod security_counter;
pub mod sign;

pub use crate::{Result, dut, hooks};

/// The board as `microtun-boot-tools-esp` knows it.
pub type ToolsBoard = microtun_boot_tools_esp::board::Board;

/// Where BLOCK0.SECURE_VERSION reads back. On every supported chip the
/// field sits in `EFUSE_RD_REPEAT_DATA3_REG` (controller + `0x3c`), with its
/// repeat-programming-error bits at the same position in
/// `EFUSE_RD_REPEAT_DATA_ERR3_REG` (controller + `0x188`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecureVersionEfuse {
    /// Base address of the eFuse controller.
    pub controller: u64,
    /// Position of the field's lowest bit in the read register.
    pub shift: u32,
    /// Width of the field. microtun boot burns one bit per epoch, so this
    /// is also the number of epochs.
    pub bits: u32,
}

/// Prefix of the environment variables shared by every ESP board:
/// `MICROTUN_BOOT_ESP_PORT`, `MICROTUN_BOOT_ESP_FLASH_TIMEOUT_SECS`. A
/// suite runs against one board, so the chip need not be in the name
/// (as with `MICROTUN_BOOT_ESP_BOARD` in `microtun-boot-tools-esp`).
pub const ENV_PREFIX: &str = "MICROTUN_BOOT_ESP";

/// What the harness needs about one ESP board beyond [`ToolsBoard`].
pub struct EspBoard {
    pub tools: &'static ToolsBoard,
    /// Rust target of the board's bootloader and firmware crates.
    pub rust_target: &'static str,
    /// Completes "set `<prefix>_PORT` to …".
    pub port_hint: &'static str,
    /// The chip's native USB pins, for probe diagnostics.
    pub usb_pins: &'static str,
    pub secure_version: SecureVersionEfuse,
}

/// One ESP board under test: its description, the harness [`Board`] built
/// from it, and per-process caches of its builds and partition table.
pub struct EspTarget {
    spec: EspBoard,
    board: LazyLock<Board>,
    cache: Cache,
}

struct Cache {
    partitions: OnceLock<Result<partitions::Partitions>>,
    bootloaders: [OnceLock<artifacts::Bootloader>; Variant::COUNT],
    firmware_elf: OnceLock<Vec<u8>>,
    firmware_image: OnceLock<esp_image::EspImage>,
}

impl EspTarget {
    /// `board` builds the harness board, normally
    /// `|| TARGET.harness_board()` on the static being defined.
    pub const fn new(spec: EspBoard, board: fn() -> Board) -> Self {
        Self {
            spec,
            board: LazyLock::new(board),
            cache: Cache {
                partitions: OnceLock::new(),
                bootloaders: [const { OnceLock::new() }; Variant::COUNT],
                firmware_elf: OnceLock::new(),
                firmware_image: OnceLock::new(),
            },
        }
    }

    /// The harness board: what [`flash::FlashImage`] and [`dut::Dut`] carry.
    pub fn board(&'static self) -> &'static Board {
        &self.board
    }

    /// Chip facts from the host tools' board table.
    pub fn chip(&self) -> &'static microtun_boot_tools_esp::board::ChipInfo {
        self.tools.chip
    }

    /// Build the harness board. Its slot size comes from the partition
    /// table the fixtures program, so this reads (and caches) that table.
    pub fn harness_board(&'static self) -> Board {
        let chip = self.chip();
        let table = partitions::read(self)
            .unwrap_or_else(|e| panic!("{} partition table: {e}", chip.display));
        Board {
            name: self.tools.name,
            dir: self.tools.dir_name,
            chip: chip.display,
            env_prefix: ENV_PREFIX,
            target: self.rust_target,
            flash_size: self.tools.flash_size as usize,
            console: Console {
                port_hint: self.port_hint,
                open: open_console,
                // The chip may re-enumerate its native USB device during resets.
                reenumerates: true,
            },
            probe: None,
            signing: Signing {
                header_size: chip.header_size,
                slot_size: table.slots.primary.size,
                sector: chip.erase_size,
                vid: "microtun.dev",
                // The MCUboot class the bootloader's policy pins (`esp32c3`, ...).
                cid: chip.name,
                rom_fixed: None,
            },
        }
    }
}

impl Deref for EspTarget {
    type Target = EspBoard;
    fn deref(&self) -> &EspBoard {
        &self.spec
    }
}

/// Every ESP target compiled into this build.
pub fn targets() -> impl Iterator<Item = &'static EspTarget> {
    [
        #[cfg(feature = "esp32c3-supermini")]
        &crate::esp32c3::TARGET,
        #[cfg(feature = "esp32c5-mini")]
        &crate::esp32c5::TARGET,
        #[cfg(feature = "esp32c6-supermini")]
        &crate::esp32c6::TARGET,
    ]
    .into_iter()
}

/// The ESP target named `name` (`esp32c5-mini`), if compiled in.
pub fn target_named(name: &str) -> Option<&'static EspTarget> {
    targets().find(|target| target.tools.name == name)
}

/// The ESP target behind a harness board, if it is one. Board names are
/// unique, and comparing them does not force the board's lazy setup.
pub fn target_of(board: &Board) -> Option<&'static EspTarget> {
    target_named(board.name)
}

/// Open the console with both modem lines released. Deassert RTS before DTR
/// so the intermediate state cannot accidentally trigger a core reset.
fn open_console(name: &str) -> serialport::Result<crate::NativeSerialPort> {
    use serialport::SerialPort;
    let mut port = crate::dut::open_serial(name)?;
    port.write_request_to_send(false)?;
    port.write_data_terminal_ready(false)?;
    Ok(port)
}

/// Control-line transitions for the native USB-Serial/JTAG CDC interface,
/// not the conventional external USB-to-UART modem pin wiring.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UsbControl {
    Dtr(bool),
    Rts(bool),
    PauseMs(u64),
}

fn flash_boot_sequence(
    mut apply: impl FnMut(UsbControl) -> serialport::Result<()>,
) -> serialport::Result<()> {
    // 0/0 clears the USB CDC download flag; 1/0 resets; 0/0 releases reset.
    // The short gaps let the peripheral and OS CDC driver settle between
    // control-line transitions. Never assert DTR during this sequence.
    for step in [
        UsbControl::Dtr(false),
        UsbControl::Rts(false),
        UsbControl::PauseMs(50),
        UsbControl::Rts(true),
        UsbControl::PauseMs(100),
        UsbControl::Rts(false),
    ] {
        apply(step)?;
    }
    Ok(())
}

/// Boot after the Rust flasher's no-reset operation. The native
/// USB-Serial/JTAG controller (identical on the ESP32-C3, ESP32-C5 and ESP32-C6)
/// interprets CDC control lines specially: DTR=false, RTS=false clears the
/// download request; RTS=true with DTR=false resets the core; RTS=false
/// releases it to boot the signed SPI image.
///
/// In HIL this runs after the serial reader starts capturing output. It also
/// handles subsequent resets using the same port (without another process or
/// a target definition). A software reset can leave ROM download mode latched,
/// resulting in `DOWNLOAD(USB/UART0/1)` followed by `waiting for download`.
/// This cannot override a physically asserted BOOT strap.
pub fn boot_from_download<P: serialport::SerialPort>(port: &mut P) -> serialport::Result<()> {
    flash_boot_sequence(|step| match step {
        UsbControl::Dtr(state) => port.write_data_terminal_ready(state),
        UsbControl::Rts(state) => port.write_request_to_send(state),
        UsbControl::PauseMs(ms) => {
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Ok(())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_sequence_never_asserts_dtr_and_ends_released() {
        let mut steps = Vec::new();
        flash_boot_sequence(|step| {
            steps.push(step);
            Ok(())
        })
        .unwrap();
        assert!(!steps.contains(&UsbControl::Dtr(true)));
        assert_eq!(steps.last(), Some(&UsbControl::Rts(false)));
        assert!(steps.contains(&UsbControl::Rts(true)));
    }

    #[test]
    fn every_target_is_found_by_name_and_matches_its_tools_board() {
        for target in targets() {
            assert!(std::ptr::eq(
                target_named(target.tools.name).unwrap(),
                target
            ));
            let board = target.board();
            assert!(std::ptr::eq(target_of(board).unwrap(), target));
            assert_eq!(board.name, target.tools.name);
            assert_eq!(board.dir, target.tools.dir_name);
            assert_eq!(board.flash_size, target.tools.flash_size as usize);
            assert_eq!(board.signing.cid, target.chip().name);
            assert_eq!(board.env("PORT"), "MICROTUN_BOOT_ESP_PORT");
            // The SECURE_VERSION field fits its 32-bit read register.
            let efuse = target.secure_version;
            assert!(efuse.bits > 0 && efuse.shift + efuse.bits <= 32);
        }
        assert!(target_named("esp32c4").is_none());
        assert!(target_named("esp32c5").is_none());
        assert!(target_of(&crate::TEST_BOARD).is_none());
    }

    /// [`crate::artifacts`] finds the outputs under the Rust target that the
    /// board directory's Cargo config builds for.
    #[test]
    fn rust_target_matches_the_board_directory() {
        for target in targets() {
            let path = target.tools.dir().join(".cargo/config.toml");
            let config = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(
                config.contains(&format!("target = \"{}\"", target.rust_target)),
                "{} does not build for {}",
                path.display(),
                target.rust_target
            );
        }
    }
}
