//! ESP32-C5-DevKitC-1: 4 MiB configured flash, second stage at `0x2000` (the ROM's
//! key manager owns `0x0..0x2000`), console and fixture flashing over the
//! chip's USB-Serial-JTAG (the "ESP32-C5 USB" connector, GPIO13/GPIO14).
//!
//! Everything else is the shared ESP back-end in [`crate::esp`], called with
//! [`TARGET`].

use crate::esp::{EspBoard, EspTarget, SecureVersionEfuse};

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

pub static TARGET: EspTarget = EspTarget::new(
    EspBoard {
        tools: &microtun_boot_tools_esp::board::ESP32C5_DEVKITC_1,
        rust_target: "riscv32imac-unknown-none-elf",
        port_hint: "the board's \"ESP32-C5 USB\" USB-Serial-JTAG port \
                    (preferably /dev/serial/by-id/...), not the USB-to-UART port",
        usb_pins: "GPIO13 (D-) / GPIO14 (D+)",
        secure_version: SECURE_VERSION,
    },
    || TARGET.harness_board(),
);

const _: () =
    assert!(layout::FLASH_SIZE == microtun_boot_tools_esp::board::ESP32C5_DEVKITC_1.flash_size);
const _: () = assert!(layout::ERASE_SIZE == crate::flash::SECTOR);
