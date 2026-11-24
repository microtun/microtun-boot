//! Facts about the chip selected by this crate's chip feature.
//!
//! The values shared with linker scripts, board layout crates and host tools
//! come from `chips/<chip>`; the Rust-only register addresses and memory
//! regions live in the chip's module. Nothing here depends on the board:
//! flash size and partition geometry come from the partition table.

#[cfg(any(
    all(feature = "esp32c3", feature = "esp32c5"),
    all(feature = "esp32c3", feature = "esp32c6"),
    all(feature = "esp32c5", feature = "esp32c6"),
))]
compile_error!(
    "microtun-boot-esp: the `esp32c3`, `esp32c5` and `esp32c6` features are mutually exclusive"
);
#[cfg(not(any(feature = "esp32c3", feature = "esp32c5", feature = "esp32c6")))]
compile_error!(
    "microtun-boot-esp: enable exactly one chip feature (`esp32c3`, `esp32c5` or `esp32c6`)"
);

macro_rules! shared_constants {
    ($krate:ident) => {
        pub use $krate::{
            BL_DRAM_BASE, BOOT_INFO_ADDRESS, BOOT_INFO_SIZE, ERASE_SIZE, HEADER_SIZE,
            IDF_IRAM_LOADER_START, IRAM_DRAM_OFFSET, IRAM_HANDOFF_SIZE, LOADER_PLAN_ADDRESS,
            LOADER_STACK_BOTTOM, LOADER_STACK_TOP, MMU_PAGE_SIZE, PARTITION_TABLE_SIZE,
            PROTECTED_START, ROM_DATA_START, RTC_FAST_BASE, RTC_FAST_SIZE,
        };
    };
}

#[cfg(feature = "esp32c3")]
shared_constants!(microtun_boot_chip_esp32c3);
#[cfg(feature = "esp32c3")]
mod esp32c3;
#[cfg(feature = "esp32c3")]
pub use esp32c3::*;

#[cfg(feature = "esp32c5")]
shared_constants!(microtun_boot_chip_esp32c5);
#[cfg(feature = "esp32c5")]
mod esp32c5;
#[cfg(feature = "esp32c5")]
pub use esp32c5::*;

#[cfg(feature = "esp32c6")]
shared_constants!(microtun_boot_chip_esp32c6);
#[cfg(feature = "esp32c6")]
mod esp32c6;
#[cfg(feature = "esp32c6")]
pub use esp32c6::*;

// Chip-independent consistency of the shared values.
const _: () = {
    assert!(ERASE_SIZE > 0);
    assert!(MMU_PAGE_SIZE > 0 && MMU_PAGE_SIZE.is_multiple_of(ERASE_SIZE));
    // The ESP image after the MCUboot header must stay MMU-page aligned when
    // a slot starts on a page boundary minus the header.
    assert!(HEADER_SIZE > 0 && HEADER_SIZE.is_multiple_of(ERASE_SIZE));
    assert!(HEADER_SIZE < MMU_PAGE_SIZE);
    assert!(PARTITION_TABLE_SIZE > 0 && PARTITION_TABLE_SIZE.is_multiple_of(ERASE_SIZE));
    assert!(PROTECTED_START > BL_DRAM_BASE);
    // microtun boot's regular stack ends below the protected handoff region; the
    // new image's plan sits above the handoff and below the loader's stack.
    assert!(PROTECTED_START + IRAM_HANDOFF_SIZE <= LOADER_PLAN_ADDRESS);
    assert!(LOADER_PLAN_ADDRESS.is_multiple_of(16));
    assert!(LOADER_PLAN_ADDRESS + 0x800 <= LOADER_STACK_BOTTOM);
    assert!(LOADER_STACK_BOTTOM.is_multiple_of(16));
    assert!(LOADER_STACK_TOP.is_multiple_of(16));
    assert!(LOADER_STACK_BOTTOM + 0x1000 <= LOADER_STACK_TOP);
    assert!(LOADER_STACK_TOP < BOOT_INFO_ADDRESS);
    assert!(BOOT_INFO_ADDRESS.is_multiple_of(4));
    assert!(BOOT_INFO_SIZE as usize == microtun_boot_core::shared_data::BOOT_INFO_SIZE);
    assert!(BOOT_INFO_ADDRESS + BOOT_INFO_SIZE == ROM_DATA_START);
};
