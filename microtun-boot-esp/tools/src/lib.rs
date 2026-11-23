//! ESP32 host tooling: ESP image checks and secure padding, Secure Boot V2
//! signing, ESP-IDF partition tables and the IDF boot chain.
#![forbid(unsafe_code)]

pub mod chain;
pub mod elf;
pub mod flash;
pub mod image;
pub mod layout;
pub mod merge;
pub mod sbv2;

use anyhow::Result;
use microtun_boot_build::tools::{mcuboot::Geometry, signer::DevKey};

/// MCUboot geometry of the board's primary slot (`boards/esp32c3-supermini/layout/partitions.csv`).
pub fn geometry() -> Result<Geometry> {
    use microtun_boot_layout_esp32c3_supermini::*;
    let table = layout::default_layout()?;
    Ok(Geometry {
        header: HEADER_SIZE,
        slot: table.region("image-0")?.1,
        erase: ERASE_SIZE,
        write: 4,
        rom_fixed: None,
        class: "esp32c3",
    })
}

/// Local development RSA-3072 Secure Boot V2 key (microtun-signer local store).
pub const DEV_KEY: DevKey = DevKey {
    key_id: "microtun-boot-local-esp32c3-sbv2-rsa3072",
    env: "ESP_SBV2_KEY_ID",
};
