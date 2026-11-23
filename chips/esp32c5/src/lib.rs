#![no_std]
//! Board-independent ESP32-C5 hardware and microtun boot ABI constants.
//!
//! The C5 has one unified SRAM bus (no separate IRAM/DRAM aliases), so
//! `IRAM_DRAM_OFFSET` is zero and the IDF loader segment address is the
//! protected-region start itself. Addresses follow ESP-IDF v5.5.5
//! `components/bootloader/subproject/main/ld/esp32c5/bootloader.ld`.

pub const CHIP: &str = "esp32c5";
/// Flash offset at which the mask ROM looks for the second-stage bootloader.
/// The first two sectors are reserved for the C5 key manager.
pub const ROM_BOOTLOADER_OFFSET: u32 = 0x00002000;
pub const ERASE_SIZE: u32 = 0x00001000;
pub const HEADER_SIZE: u32 = 0x00001000;
pub const MMU_PAGE_SIZE: u32 = 0x00010000;
pub const PARTITION_TABLE_SIZE: u32 = 0x00001000;
pub const IRAM_DRAM_OFFSET: u32 = 0x00000000;
pub const BL_DRAM_BASE: u32 = 0x40800000;
/// IDF `bootloader_iram_loader_seg_start`: everything from here up to the
/// ROM's data is reclaimed SRAM that microtun boot reserves for the handoff.
pub const PROTECTED_START: u32 = 0x4084e5a0;
pub const IRAM_HANDOFF_SIZE: u32 = 0x00001000;
pub const IDF_IRAM_LOADER_START: u32 = 0x4084e5a0;
pub const BOOT_INFO_ADDRESS: u32 = 0x4085e4a0;
pub const BOOT_INFO_SIZE: u32 = 0x00000100;
/// Start of the mask ROM's `.bss`/`.data`, which must never be overwritten.
pub const ROM_DATA_START: u32 = 0x4085e5a0;
/// LP SRAM (RTC fast memory), 16 KiB.
pub const RTC_FAST_BASE: u32 = 0x50000000;
pub const RTC_FAST_SIZE: u32 = 0x00004000;
pub const LOADER_PLAN_ADDRESS: u32 = 0x40855000;
pub const LOADER_STACK_BOTTOM: u32 = 0x40856000;
pub const LOADER_STACK_TOP: u32 = 0x40858000;
