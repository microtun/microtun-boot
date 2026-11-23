//! ESP image-acceptance policy for the selected chip.
//!
//! The VID/CID are Microtun-derived identifiers, generated exactly as
//! `imgtool sign --vid microtun.dev --cid <chip>` (`esp32c3` or `esp32c5`).
//! Keep both values synchronized with the image-signing defaults.
//! Both the bootloader and the firmware updater must enforce the same policy.

pub use microtun_boot_core::Policy;

/// UUIDv5(DNS, "microtun.dev") — shared Microtun vendor ID.
pub const IMAGE_VID: [u8; 16] = [
    0xb3, 0x6b, 0x1d, 0x87, 0x77, 0x70, 0x53, 0xa0, 0xa5, 0x23, 0x9d, 0x81, 0x1b, 0x3b, 0x95, 0x5f,
];
/// UUIDv5(IMAGE_VID, "esp32c3") — ESP32C3 firmware class.
#[cfg(feature = "esp32c3")]
pub const IMAGE_CID: [u8; 16] = [
    0xfc, 0xc0, 0x08, 0x85, 0x1b, 0xbf, 0x51, 0xa8, 0xa0, 0x5f, 0x9b, 0x77, 0xe0, 0x3c, 0xe4, 0x73,
];
/// UUIDv5(IMAGE_VID, "esp32c5") — ESP32C5 firmware class.
#[cfg(feature = "esp32c5")]
pub const IMAGE_CID: [u8; 16] = [
    0xdf, 0xf2, 0xe3, 0x18, 0xd6, 0x35, 0x55, 0xa5, 0xb8, 0xb1, 0x83, 0x25, 0x4c, 0xbd, 0x59, 0xbc,
];

/// Chip-specific constructors for [`Policy`].
pub trait PolicyExt {
    /// Require the signed image's protected VID/CID TLVs and security counter.
    #[cfg(feature = "esp32c3")]
    fn esp32c3() -> Self;
    /// Require the signed image's protected VID/CID TLVs and security counter.
    #[cfg(feature = "esp32c5")]
    fn esp32c5() -> Self;
}

fn chip_policy() -> Policy {
    Policy {
        min_security_counter: Some(0),
        vid: Some(IMAGE_VID),
        cid: Some(IMAGE_CID),
        ..Policy::default()
    }
}

impl PolicyExt for Policy {
    #[cfg(feature = "esp32c3")]
    fn esp32c3() -> Self {
        chip_policy()
    }
    #[cfg(feature = "esp32c5")]
    fn esp32c5() -> Self {
        chip_policy()
    }
}
