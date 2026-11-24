#![no_std]
//! ESP32-C6 SuperMini board configuration; chip constants live in chips/esp32c6.
//! Target code reads partitions from flash; host tools read ESP-IDF CSV.

pub use microtun_boot_chip_esp32c6::*;

/// Capacity of the flash on this board (the ESP32-C6FH4's 4 MiB in-package
/// flash); not a chip property.
pub const FLASH_SIZE: u32 = 0x0040_0000;

const _: () = {
    assert!(ERASE_SIZE > 0 && FLASH_SIZE.is_multiple_of(ERASE_SIZE));
    assert!(FLASH_SIZE.is_multiple_of(MMU_PAGE_SIZE));
};
