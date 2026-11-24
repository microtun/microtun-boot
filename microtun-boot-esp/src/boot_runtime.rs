//! `esp-hal` boot-time services for the ESP third-stage bootloader.
//!
//! This module is intentionally not used by the standalone `bl-loader` RAM
//! payload: that payload overwrites the memory holding the HAL and must use
//! ROM entry points after the handoff has begun.

use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    peripherals::{Peripherals, RTC_TIMER, TIMG0},
    rtc_cntl::Rwdt,
    time::Duration,
};

/// Initialise the HAL for the factory microtun boot application.
///
/// Clear the previous SRAM handoff *before* HAL startup, as a reset may leave
/// stale boot information behind. `esp_hal::init` configures clocks and turns
/// off the ROM's watchdogs; the TIMG0 flash-boot mode bit additionally needs
/// to be cleared before a lengthy validation or flash swap.
///
/// Call exactly once, at the beginning of the third-stage bootloader (not in
/// an OTA application which has already initialised the HAL).
pub fn init() -> Peripherals {
    crate::BootInfo::invalidate();
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    finish_flashboot_protection();
    peripherals
}

/// Clear only the TIMG0 flash-boot protection bit. The HAL's standard WDT
/// driver can enable/disable the complete MWDT, but has no operation for this
/// ROM boot-mode bit. Leave the rest of the watchdog configuration unchanged.
fn finish_flashboot_protection() {
    let tg0 = TIMG0::regs();
    if tg0.wdtconfig0().read().wdt_flashboot_mod_en().bit_is_set() {
        // The PAC reset value is the WDT unlock key (C3, C5, C6: 0x50D83AA1).
        tg0.wdtwprotect().reset();
        tg0.wdtconfig0()
            .modify(|_, w| w.wdt_flashboot_mod_en().clear_bit());
        tg0.wdtconfig0()
            .modify(|_, w| w.wdt_conf_update_en().set_bit());
        tg0.wdtwprotect().write(|w| {
            // SAFETY: zero is the documented watchdog lock value.
            unsafe { w.wdt_wkey().bits(0) }
        });
    }
}

/// Deadline between two completed flash operations while the bootloader
/// validates or swaps images, and for application startup after the handoff.
pub const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(15);

/// Arm the RTC watchdog at the start of the third-stage bootloader, right
/// after [`init`] (which disables the watchdog inherited from the ROM).
///
/// Feed it after every completed flash operation by wrapping the flash in
/// [`microtun_boot_core::flash::ProgressFlash`]: validation and restartable swaps
/// of any slot size then stay within the deadline, while a stuck flash or
/// eFuse operation resets the device and the interrupted operation resumes
/// on the next boot. Feed it once more immediately before the handoff. The
/// driver need not be polled during the SRAM handoff; its hardware timer keeps
/// running while the loader overwrites the bootloader's DRAM. The application
/// must take responsibility for the watchdog after the jump.
pub fn start_watchdog(timer: RTC_TIMER<'_>) -> Rwdt {
    crate::watchdog::start_rtc(timer, WATCHDOG_TIMEOUT)
}

/// Give native USB-Serial-JTAG diagnostics time to drain, then reset.
///
/// Only for the ordinary HAL-backed bootloader, before the destructive handoff.
pub fn reset_after_delay() -> ! {
    crate::BootInfo::invalidate();
    Delay::new().delay_millis(2_000);
    esp_hal::system::software_reset()
}

/// Randomness for the delays in front of fault-hardened checks on ESP32-C3
/// and ESP32-C6: esp-hal's TRNG, with the SAR ADC entropy source running.
///
/// TODO(esp32c5): esp-hal 1.2.2 has no C5 TRNG entropy source, and ESP-IDF's
/// second stage switches off its SAR ADC noise source before microtun boot.
/// Restore this service on C5 only when a real entropy source is supported
/// and verified. Until then the hardened checks run without random delays.
///
/// Call [`BootRng::release`] before the handoff.
#[cfg(any(feature = "esp32c3", feature = "esp32c6"))]
pub struct BootRng<'d> {
    trng: esp_hal::rng::Trng,
    source: esp_hal::rng::TrngSource<'d>,
}

#[cfg(any(feature = "esp32c3", feature = "esp32c6"))]
impl<'d> BootRng<'d> {
    /// Take the RNG and its entropy source so no other driver reconfigures them.
    pub fn new(rng: esp_hal::peripherals::RNG<'d>, adc: esp_hal::peripherals::ADC1<'d>) -> Self {
        let source = esp_hal::rng::TrngSource::new(rng, adc);
        let trng = esp_hal::rng::Trng::try_new().expect("TRNG entropy source");
        Self { trng, source }
    }

    /// Stop the entropy source (the SAR ADC) before the RAM handoff.
    pub fn release(self) {
        let Self { trng, source } = self;
        // The HAL refuses to disable the source while a TRNG user is alive.
        drop(trng);
        drop(source);
    }

    fn word(&mut self) -> u32 {
        rand::RngCore::next_u32(&mut self.trng)
    }
}

#[cfg(any(feature = "esp32c3", feature = "esp32c6"))]
impl rand::RngCore for BootRng<'_> {
    fn next_u32(&mut self) -> u32 {
        self.word()
    }
    fn next_u64(&mut self) -> u64 {
        u64::from(self.word()) << 32 | u64::from(self.word())
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        for chunk in dest.chunks_mut(4) {
            chunk.copy_from_slice(&self.word().to_le_bytes()[..chunk.len()]);
        }
    }
}

#[cfg(any(feature = "esp32c3", feature = "esp32c6"))]
impl rand::CryptoRng for BootRng<'_> {}
