//! Checked boot context in the 256-byte NOLOAD reservation shared by both linkers.
pub use microtun_boot_core::shared_data::{
    SharedData, SharedDataError, boot_save_shared_data, wire_words,
};

use crate::{BOOT_INFO_ADDRESS, BOOT_INFO_SIZE as PLATFORM_BOOT_INFO_SIZE};
use microtun_boot_core::shared_data::{BOOT_INFO_SIZE, BOOT_INFO_WORDS};

pub const ADDRESS: u32 = BOOT_INFO_ADDRESS;
pub const RESERVED_SIZE: usize = BOOT_INFO_SIZE;
const _: () = assert!(
    RESERVED_SIZE == PLATFORM_BOOT_INFO_SIZE as usize,
    "chip description and microtun-boot-core disagree on the boot-info size"
);
pub const WORDS: usize = BOOT_INFO_WORDS;
pub type BootData = SharedData<RESERVED_SIZE>;

/// Owned boot context; no references into reserved SRAM are retained.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootInfo(microtun_boot_core::shared_data::BootInfo);

impl BootInfo {
    /// Decode the microtun boot handoff TLVs on a host or target. Invalid or
    /// unsupported handoffs must never be used to guess which slot can be
    /// overwritten.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SharedDataError> {
        microtun_boot_core::shared_data::BootInfo::from_handoff(bytes).map(Self)
    }

    /// [`from_bytes`](Self::from_bytes) for the reservation read as words.
    pub fn from_words(words: &[u32; WORDS]) -> Result<Self, SharedDataError> {
        microtun_boot_core::shared_data::BootInfo::from_handoff_words(words).map(Self)
    }
}

impl core::ops::Deref for BootInfo {
    type Target = microtun_boot_core::shared_data::BootInfo;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
