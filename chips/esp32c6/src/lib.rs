#![no_std]
//! Board-independent ESP32-C6 hardware and microtun boot ABI constants.
//!
//! Like the C5, the C6 has one unified HP SRAM bus (no separate IRAM/DRAM
//! aliases), so `IRAM_DRAM_OFFSET` is zero and the IDF loader segment address
//! is the protected-region start itself. Addresses follow ESP-IDF v5.5.5
//! `components/bootloader/subproject/main/ld/esp32c6/bootloader.ld`.

pub const CHIP: &str = "esp32c6";
/// Flash offset at which the mask ROM looks for the second-stage bootloader.
pub const ROM_BOOTLOADER_OFFSET: u32 = 0x00000000;
pub const ERASE_SIZE: u32 = 0x00001000;
pub const HEADER_SIZE: u32 = 0x00001000;
pub const MMU_PAGE_SIZE: u32 = 0x00010000;
pub const PARTITION_TABLE_SIZE: u32 = 0x00001000;
pub const IRAM_DRAM_OFFSET: u32 = 0x00000000;
pub const BL_DRAM_BASE: u32 = 0x40800000;
/// IDF `bootloader_iram_loader_seg_start`: everything from here up to the
/// ROM's data is reclaimed SRAM that microtun boot reserves for the handoff.
pub const PROTECTED_START: u32 = 0x4086e610;
pub const IRAM_HANDOFF_SIZE: u32 = 0x00001000;
pub const IDF_IRAM_LOADER_START: u32 = 0x4086e610;
pub const BOOT_INFO_ADDRESS: u32 = 0x4087e510;
pub const BOOT_INFO_SIZE: u32 = 0x00000100;
/// Start of the mask ROM's `.bss`/`.data`, which must never be overwritten.
pub const ROM_DATA_START: u32 = 0x4087e610;
/// LP SRAM (RTC fast memory), 16 KiB.
pub const RTC_FAST_BASE: u32 = 0x50000000;
pub const RTC_FAST_SIZE: u32 = 0x00004000;
pub const LOADER_PLAN_ADDRESS: u32 = 0x40875000;
pub const LOADER_STACK_BOTTOM: u32 = 0x40876000;
pub const LOADER_STACK_TOP: u32 = 0x40878000;
