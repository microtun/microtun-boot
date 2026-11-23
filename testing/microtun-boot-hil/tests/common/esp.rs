//! Shared fixtures for the runtime scenarios on a real ESP32 board (C3 or
//! C5, whichever board feature is enabled). Everything board-specific comes
//! from [`ESP`]; the scenarios themselves are shared with RP2350.

pub use microtun_boot_core::Slot;
pub use microtun_boot_hil::esp::{
    artifacts::Variant,
    dut::Dut,
    esp_image::EspImage,
    flash::{FlashImage, Request},
    hooks::{Behaviour, HookConfig},
};
pub use microtun_boot_hil::scenario::{BOOT, QUIET, behave, expect_running, field, version};

use microtun_boot_hil::esp::{EspTarget, artifacts, flash};

/// The board under test. (`mod.rs` rejects enabling both ESP boards.)
#[cfg(feature = "esp32c3-supermini")]
pub static ESP: &EspTarget = &microtun_boot_hil::esp32c3::TARGET;
#[cfg(all(feature = "esp32c5-devkitc-1", not(feature = "esp32c3-supermini")))]
pub static ESP: &EspTarget = &microtun_boot_hil::esp32c5::TARGET;

/// Number of SECURE_VERSION epochs (eFuse bits) on the board's chip.
#[cfg(feature = "esp32c3-supermini")]
pub const SECURITY_COUNTER_CAPACITY: u32 = microtun_boot_hil::esp32c3::SECURE_VERSION.bits;

/// imgtool for the board's slot geometry, with the RP2350/QEMU signature.
pub mod sign {
    pub use microtun_boot_hil::esp::sign::{Key, SignOptions};

    pub fn sign(payload: &[u8], opts: &SignOptions) -> microtun_boot_hil::Result<Vec<u8>> {
        microtun_boot_hil::esp::sign::sign(super::ESP, payload, opts)
    }
}
pub use sign::{Key, SignOptions};

/// The HIL fixture (always with test hooks) as an ESP image.
pub fn payload() -> EspImage {
    artifacts::firmware_image(ESP).clone()
}

/// The firmware signed with the trusted key as `version` (e.g. "2.0.0").
pub fn signed(version: &str) -> Vec<u8> {
    sign::sign(
        artifacts::firmware_image(ESP).bytes(),
        &SignOptions::new(version),
    )
    .unwrap()
}

/// Erased slots, both signed boot stages, the board partition table and a hook
/// record in which every image confirms itself and idles.
pub fn blank(variant: Variant) -> FlashImage {
    let flash = flash::compose(ESP, artifacts::bootloader(ESP, variant)).unwrap();
    flash.configure(&HookConfig::default());
    flash
}

/// A device with a good, confirmed 1.0.0 image in the primary slot.
pub fn factory(variant: Variant) -> FlashImage {
    let flash = blank(variant);
    flash
        .install(Slot::Primary, &signed("1.0.0"))
        .confirm_raw(Slot::Primary);
    flash
}

// ----- malformed ESP payloads ---------------------------------------------------

/// An ESP image the bootloader's platform validator must refuse.
pub struct Malformed {
    pub name: &'static str,
    pub image: EspImage,
}

/// One smoke mutation targeting the ESP protected loader: the first
/// instruction-RAM segment is moved onto the protected handoff region (on
/// the ESP32-C5 instruction and data RAM are one bus, so this may be any RAM
/// segment). Re-signing isolates the executable-format check from signature
/// validation. The address ranges come from the host tools' chip table; the
/// on-device loader remains responsible for validating every candidate.
pub fn smoke_malformed(img: &EspImage) -> Malformed {
    let chip = ESP.chip();
    let iram = img.segment_in(chip.iram);
    Malformed {
        name: "iram-over-protected-loader",
        image: img
            .clone()
            .set_segment_vaddr(iram, chip.protected_start + chip.iram_dram_offset),
    }
}

// Console differences belong here; the scenarios are shared with RP2350.
pub const PLATFORM_REJECTED: &str = "rejecting application executable";
pub const NO_EXECUTABLE: &str = "unable to find a bootable image";
pub fn max_image_size(flash: &FlashImage) -> String {
    flash
        .layout
        .max_image_size::<microtun_boot_hil::flash::Partition>()
        .unwrap()
        .to_string()
}
pub const STARTING_IMAGE: &str = "jumping to";
pub fn secondary_start(flash: &FlashImage) -> String {
    format!(
        "ESP image at {:#x}",
        flash.layout.secondary.offset + microtun_boot_hil::esp::sign::header_size(ESP)
    )
}
pub const NORMAL_FIRMWARE: &[&str] = &[
    "running image 1.0.0+0 in Primary, mode SwapScratch",
    "application healthy; image confirmed; USB-Serial-JTAG log console ready",
];
pub fn wrong_rom_fixed(flash: &FlashImage) -> u32 {
    flash.layout.primary.offset
}

pub fn assert_watchdog_reset(dut: &Dut, from: usize) {
    // esp-hal's reset reasons distinguish digital-core and full system RTC
    // (LP on the ESP32-C5) watchdog resets, with the same names on both
    // chips. Neither software nor probe resets qualify.
    let (_, hello) = dut.expect_from(from, "@fbt hello", BOOT).unwrap();
    assert!(
        matches!(
            field(&hello, "reset"),
            Some("Some(CoreRtcWdt)" | "Some(SysRtcWdt)")
        ),
        "expected RTC watchdog reset: {hello}"
    );
}
