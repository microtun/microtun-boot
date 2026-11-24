//! ESP32-C5-DevKitC-1: second stage at `0x2000` (the ROM's key manager owns
//! `0x0..0x2000`), console and fixture flashing over the chip's
//! USB-Serial-JTAG (the "ESP32-C5 USB" connector, GPIO13/GPIO14).
//!
//! The board comes with 4, 8 or 16 MiB of flash. Each is its own target
//! ([`TARGET_N4`], [`TARGET_N8`], [`TARGET_N16`]), enabled by the
//! `esp32c5-devkitc-1-n4`, `-n8` or `-n16` feature; they build the same
//! binaries and differ only in their partition table and flash size.
//!
//! Everything else is the shared ESP back-end in [`crate::esp`].

use crate::esp::{EspBoard, EspTarget, SecureVersionEfuse, ToolsBoard};

/// Hardware/RAM ABI shared with the target; no partition locations.
pub use microtun_boot_layout_esp32c5_devkitc_1 as layout;

/// SECURE_VERSION is 9 bits at bits 9..17 of BLOCK0 read word 4 (BLOCK0
/// bits 137..145), per ESP-IDF v5.5.5's eFuse table, esp-hal and
/// the retained C5 counter implementation (temporarily disabled). Keep this
/// geometry for future HIL coverage; C5 smoke signing does not read it.
/// The esp32c5 PAC places
/// `EFUSE_RD_REPEAT_DATA3_REG` at controller `0x600B_4800` + `0x3c` and
/// its error register at + `0x188`, as on the ESP32-C3.
pub const SECURE_VERSION: SecureVersionEfuse = SecureVersionEfuse {
    controller: 0x600b_4800,
    shift: 9,
    bits: 9,
};

/// What the harness adds to the tools' description of a C5 variant.
const fn board(tools: &'static ToolsBoard) -> EspBoard {
    EspBoard {
        tools,
        rust_target: "riscv32imac-unknown-none-elf",
        port_hint: "the board's \"ESP32-C5 USB\" USB-Serial-JTAG port \
                    (preferably /dev/serial/by-id/...), not the USB-to-UART port",
        usb_pins: "GPIO13 (D-) / GPIO14 (D+)",
        secure_version: SECURE_VERSION,
    }
}

/// ESP32-C5-DevKitC-1 with 4 MiB of flash.
#[cfg(feature = "esp32c5-devkitc-1-n4")]
pub static TARGET_N4: EspTarget = EspTarget::new(
    board(&microtun_boot_tools_esp::board::ESP32C5_DEVKITC_1_N4),
    || TARGET_N4.harness_board(),
);

/// ESP32-C5-DevKitC-1 with 8 MiB of flash.
#[cfg(feature = "esp32c5-devkitc-1-n8")]
pub static TARGET_N8: EspTarget = EspTarget::new(
    board(&microtun_boot_tools_esp::board::ESP32C5_DEVKITC_1_N8),
    || TARGET_N8.harness_board(),
);

/// ESP32-C5-DevKitC-1 with 16 MiB of flash.
#[cfg(feature = "esp32c5-devkitc-1-n16")]
pub static TARGET_N16: EspTarget = EspTarget::new(
    board(&microtun_boot_tools_esp::board::ESP32C5_DEVKITC_1_N16),
    || TARGET_N16.harness_board(),
);

const _: () = {
    use microtun_boot_tools_esp::board as tools;
    assert!(layout::n4::FLASH_SIZE == tools::ESP32C5_DEVKITC_1_N4.flash_size);
    assert!(layout::n8::FLASH_SIZE == tools::ESP32C5_DEVKITC_1_N8.flash_size);
    assert!(layout::n16::FLASH_SIZE == tools::ESP32C5_DEVKITC_1_N16.flash_size);
};
const _: () = assert!(layout::ERASE_SIZE == crate::flash::SECTOR);
