#![no_std]
//! ESP32-C5-DevKitC-1 board configuration; chip constants live in chips/esp32c5.
//! Target code reads partitions from flash; host tools read ESP-IDF CSV.
//!
//! The DevKitC-1 ships with modules of several flash sizes. Each is its own
//! board (`esp32c5-devkitc-1-n4`, `-n8`, `-n16`) with its own partition table
//! (`partitions-<variant>.csv`); the binaries are shared.

pub use microtun_boot_chip_esp32c5::*;

/// `esp32c5-devkitc-1-n4`: 4 MiB of flash.
pub mod n4 {
    pub const FLASH_SIZE: u32 = 0x0040_0000;
}

/// `esp32c5-devkitc-1-n8`: 8 MiB of flash.
pub mod n8 {
    pub const FLASH_SIZE: u32 = 0x0080_0000;
}

/// `esp32c5-devkitc-1-n16`: 16 MiB of flash.
pub mod n16 {
    pub const FLASH_SIZE: u32 = 0x0100_0000;
}

const _: () = {
    let mut i = 0;
    let sizes = [n4::FLASH_SIZE, n8::FLASH_SIZE, n16::FLASH_SIZE];
    while i < sizes.len() {
        assert!(ERASE_SIZE > 0 && sizes[i].is_multiple_of(ERASE_SIZE));
        assert!(sizes[i].is_multiple_of(MMU_PAGE_SIZE));
        i += 1;
    }
};
