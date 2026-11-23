#![no_std]
//! ESP32-C5-DevKitC-1 board configuration; chip constants live in chips/esp32c5.
//! Target code reads partitions from flash; host tools read ESP-IDF CSV.

pub use microtun_boot_chip_esp32c5::*;

/// Flash capacity used by the generic board target (not a chip property).
/// Use the first 4 MiB on variants with at least that much physical flash.
pub const FLASH_SIZE: u32 = 0x0040_0000;

const _: () = {
    assert!(ERASE_SIZE > 0 && FLASH_SIZE.is_multiple_of(ERASE_SIZE));
    assert!(FLASH_SIZE.is_multiple_of(MMU_PAGE_SIZE));
};
