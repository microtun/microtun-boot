//! The bootloader variants and the firmware, built once per test process by
//! [`crate::artifacts`] under `target/hil-<board>/`. The ESP-IDF second
//! stage and microtun boot factory app are both RSA-3072 signed, as on hardware.

pub use crate::artifacts::{Variant, repo_root};

use super::{
    EspTarget,
    esp_image::{self, EspImage},
};

/// A complete verified boot chain, not just microtun boot's ELF.
pub struct Bootloader {
    /// Signed microtun boot app, placed in the table's `microtun-boot` partition.
    pub image: Vec<u8>,
    /// Signed standard ESP-IDF second stage, placed at the chip's ROM
    /// second-stage offset (`0x0` on the ESP32-C3, `0x2000` on the ESP32-C5).
    pub idf_image: Vec<u8>,
}

/// The bootloader built with `variant`'s features and converted for flash.
pub fn bootloader(target: &'static EspTarget, variant: Variant) -> &'static Bootloader {
    target.cache.bootloaders[variant as usize].get_or_init(|| {
        let elf = crate::artifacts::build_bootloader(target.board(), variant);
        let temp = tempfile::tempdir().expect("boot-chain temp directory");
        let elf_path = temp.path().join("microtun-boot.elf");
        std::fs::write(&elf_path, &elf).expect("write microtun boot ELF");
        let padded = esp_image::from_elf(target, &elf)
            .and_then(EspImage::secure_pad_v2)
            .unwrap_or_else(|e| panic!("cannot create ESP image: {e}"));
        let image_path = temp.path().join("image.bin");
        std::fs::write(&image_path, padded.bytes()).expect("write padded ESP image");
        let mut settings = microtun_boot_tools_esp::chain::Settings::from_env();
        settings.no_flash = true;
        settings.prebuilt_image = Some(image_path);
        // The local development Secure Boot V2 key, whatever the shell sets.
        let board = target.tools;
        let signer = microtun_boot_build::tools::signer::Signer::local(board.dev_key.key_id);
        let chain =
            microtun_boot_tools_esp::chain::boot_chain(board, &elf_path, &settings, &signer)
                .unwrap_or_else(|e| panic!("cannot sign ESP boot chain: {e:#}"));
        let image = chain.microtun_boot;
        let idf_image = chain.stage2;
        Bootloader { image, idf_image }
    })
}

/// The example firmware ELF with built-in test hooks.
pub fn firmware_elf(target: &'static EspTarget) -> &'static [u8] {
    target
        .cache
        .firmware_elf
        .get_or_init(|| crate::artifacts::build_firmware(target.board()))
}

/// The firmware as an ESP image. Every application image in the suite is this
/// image, signed with different versions.
pub fn firmware_image(target: &'static EspTarget) -> &'static EspImage {
    target.cache.firmware_image.get_or_init(|| {
        esp_image::from_elf(target, firmware_elf(target))
            .unwrap_or_else(|e| panic!("espflash cannot convert the firmware: {e}"))
    })
}
