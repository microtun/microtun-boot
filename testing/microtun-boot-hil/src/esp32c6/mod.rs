//! ESP32-C6-MINI-1: 4 MiB flash, second stage at `0x0`, console and
//! fixture flashing over the chip's USB-Serial-JTAG (GPIO12/GPIO13).
//!
//! Everything else is the shared ESP back-end in [`crate::esp`], called with
//! [`TARGET`].

use crate::esp::{EspBoard, EspTarget, SecureVersionEfuse};

/// Hardware/RAM ABI shared with the target; no partition locations.
pub use microtun_boot_layout_esp32_c6_mini_1 as layout;

/// ESP-IDF v5.5.5 eFuse table: SECURE_VERSION is 16 bits at bits 14..29 of
/// BLOCK0 read word 4 (BLOCK0 bits 142..157), as on the ESP32-C3. The eFuse
/// controller is at `0x600B_0800`; `EFUSE_RD_REPEAT_DATA3_REG` (+ `0x3c`) and
/// `EFUSE_RD_REPEAT_ERR3_REG` (+ `0x188`) sit at the C3's offsets.
pub const SECURE_VERSION: SecureVersionEfuse = SecureVersionEfuse {
    controller: 0x600b_0800,
    shift: 14,
    bits: 16,
};

pub static TARGET: EspTarget = EspTarget::new(
    EspBoard {
        tools: &microtun_boot_tools_esp::board::ESP32_C6_MINI_1,
        rust_target: "riscv32imac-unknown-none-elf",
        port_hint: "the board's USB-Serial-JTAG port (preferably /dev/serial/by-id/...)",
        usb_pins: "GPIO12 (D-) / GPIO13 (D+)",
        secure_version: SECURE_VERSION,
    },
    || TARGET.harness_board(),
);

const _: () =
    assert!(layout::FLASH_SIZE == microtun_boot_tools_esp::board::ESP32_C6_MINI_1.flash_size);
const _: () = assert!(layout::ERASE_SIZE == crate::flash::SECTOR);
