//! ESP32-C6 memory and MMIO constants (ESP-IDF v5.5.5 / matching PAC) that
//! only Rust code needs. Values shared with linker scripts and host tools live
//! in `chips/esp32c6` (see the parent module).
//!
//! The C6 has a single HP SRAM bus: instruction and data accesses use the same
//! addresses, so the `IRAM` and `DRAM` windows are identical. Flash code and
//! read-only data also share one 16 MiB cache window and one MMU table.
pub const NAME: &str = "ESP32-C6";
pub const IMAGE_CHIP_ID: u16 = 13;
pub const IRAM: (u32, u32) = (0x4080_0000, 0x4088_0000);
pub const DRAM: (u32, u32) = IRAM;
pub const RTC_FAST: (u32, u32) = (RTC_FAST_BASE, RTC_FAST_BASE + RTC_FAST_SIZE);

use super::{RTC_FAST_BASE, RTC_FAST_SIZE};

/// ESP32-C6 flash MMU geometry: one table of 256 64 KiB pages, shared by
/// instruction and data accesses (`SOC_MMU_DI_VADDR_SHARED`). The C6 MMU page
/// size is configurable; microtun boot always uses (and the RAM loader sets)
/// 64 KiB pages.
pub mod mmu {
    pub const ENTRIES: u32 = 256;
    pub const PAGE_SIZE: u32 = super::super::MMU_PAGE_SIZE;
    pub const IROM_BASE: u32 = 0x4200_0000;
    pub const DROM_BASE: u32 = IROM_BASE;
    pub const BUS_SIZE: u32 = ENTRIES * PAGE_SIZE;
}

// The protected-RAM handoff ABI of the C6 reference layout.
const _: () = {
    use super::*;
    // The loader starts where the retired IDF second-stage loader was placed.
    // It is never a LOAD segment in microtun boot's ESP image: installed at runtime.
    assert!(IDF_IRAM_LOADER_START == 0x4086_E610);
    assert!(IRAM_DRAM_OFFSET == 0);
    assert!(PROTECTED_START == IDF_IRAM_LOADER_START);
    assert!(IRAM_HANDOFF_SIZE == 0x1000);
    assert!(MMU_PAGE_SIZE == 0x1_0000);
    assert!(RTC_FAST_BASE == 0x5000_0000 && RTC_FAST_SIZE == 0x4000);
    assert!(BL_DRAM_BASE == DRAM.0);
    // ROM .bss/.data occupy the top of SRAM and must stay untouched.
    assert!(ROM_DATA_START < DRAM.1);
    assert!(mmu::BUS_SIZE == 0x0100_0000);
};
