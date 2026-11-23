#![no_std]
//! ESP32-C5-DevKitC-1 board configuration; chip constants live in chips/esp32c5.
//! Target code reads partitions from flash; host tools read ESP-IDF CSV.

pub use microtun_boot_chip_esp32c5::*;

/// Capacity of the flash installed on this board (not a chip property): the
/// ESP32-C5-WROOM-1 on both board revisions (N8R4, N8R8) has 8 MiB.
pub const FLASH_SIZE: u32 = 0x0080_0000;

const _: () = {
    assert!(ERASE_SIZE > 0 && FLASH_SIZE.is_multiple_of(ERASE_SIZE));
    assert!(FLASH_SIZE.is_multiple_of(MMU_PAGE_SIZE));
};
