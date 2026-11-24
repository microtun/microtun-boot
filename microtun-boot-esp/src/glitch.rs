//! Glitch detector resets, re-asserted and verified by the third stage.
//!
//! * **ESP32-C3:** the analog *clock* glitch detector.
//! * **ESP32-C5:** the analog *power* (voltage) glitch detectors. The C5 has
//!   no clock glitch detector; ESP-IDF's
//!   `bootloader_ana_clock_glitch_reset_config` is a no-op there.
//!
//! The ESP32-C6 has none: ESP-IDF v5.5.5 documents its
//! `bootloader_ana_clock_glitch_reset_config` as "not supported" and enables
//! no glitch reset, so this module is not compiled for the C6.
//!
//! In both cases ESP-IDF's second stage configures the detector first; this
//! module re-asserts the reset enable and reads it back, so microtun boot does
//! not silently depend on that stage's configuration. Neither chip has a lock
//! for these settings: the application can turn them off again.

use esp_hal::efuse::ChipRevision;

/// What this boot found when enabling the detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    /// The last reset was caused by a clock or power glitch detector.
    pub reset_by_glitch: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Glitch reset is unreliable on this silicon revision.
    UnsupportedRevision(ChipRevision),
    /// The enable did not read back as set.
    NotEnabled,
}

#[cfg(feature = "esp32c3")]
pub use esp32c3::{DETECTOR, enable};
#[cfg(feature = "esp32c5")]
pub use esp32c5::{DETECTOR, enable};

#[cfg(feature = "esp32c3")]
mod esp32c3 {
    //! The C3's analog clock glitch detector resets the digital core and RTC
    //! (reset reason `SysClkGlitch`) when it sees a glitch on the clock.
    //! ESP-IDF's second stage already enables it on rev v0.3+
    //! (`bootloader_ana_reset_config`). The register sequence mirrors
    //! ESP-IDF's `bootloader_ana_clock_glitch_reset_config(true)`.
    //!
    //! It is not a power/voltage or EM glitch detector, and IDF documents no
    //! such detector for this chip.

    use super::{Error, Report};
    use esp_hal::{
        efuse::{self, ChipRevision},
        peripherals::LPWR,
        rtc_cntl::SocResetReason,
    };

    /// The detector this module enables, for logs.
    pub const DETECTOR: &str = "clock glitch";

    /// `RTC_CNTL_FIB_SEL.FIB_GLITCH_RST`: clear for software control of the
    /// glitch reset enable, instead of the analog default.
    const FIB_GLITCH_RST: u8 = 1 << 0;

    /// ESP-IDF disables glitch reset on earlier revisions: it was fixed in ECO3.
    const MIN_REVISION: ChipRevision = ChipRevision { major: 0, minor: 3 };

    /// Enable and verify the clock glitch reset. Call once, early in the
    /// third-stage bootloader, after `esp_hal::init`.
    pub fn enable() -> Result<Report, Error> {
        let report = Report {
            reset_by_glitch: matches!(
                esp_hal::system::reset_reason(),
                Some(SocResetReason::SysClkGlitch | SocResetReason::CorePwrGlitch)
            ),
        };
        let revision = efuse::chip_revision();
        if revision < MIN_REVISION {
            return Err(Error::UnsupportedRevision(revision));
        }

        let rtc = LPWR::regs();
        rtc.fib_sel().modify(|r, w| {
            // SAFETY: only FIB_GLITCH_RST changes; the brownout and super-WDT
            // selections keep their current values.
            unsafe { w.fib_sel().bits(r.fib_sel().bits() & !FIB_GLITCH_RST) }
        });
        rtc.ana_conf().modify(|_, w| w.glitch_rst_en().set_bit());

        if rtc.fib_sel().read().fib_sel().bits() & FIB_GLITCH_RST != 0
            || rtc.ana_conf().read().glitch_rst_en().bit_is_clear()
        {
            return Err(Error::NotEnabled);
        }
        Ok(report)
    }
}

#[cfg(feature = "esp32c5")]
mod esp32c5 {
    //! The C5's voltage glitch detectors (VDDPST2/3, VDDPST1, VDDA3, VDDA8)
    //! reset the chip (reset reason `PowerGlitch`). ESP-IDF's second stage
    //! (`bootloader_power_glitch_reset_config(true)`) powers up only the
    //! VDDPST detector over the analog I2C bus, hands the reset enables to
    //! software (`LP_ANA_FIB_ENABLE`) and enables all four resets. This
    //! module re-asserts and verifies the two `LP_ANA` register fields; the
    //! analog detector power selection stays as IDF left it.

    use super::{Error, Report};
    use esp_hal::{
        efuse::{self, ChipRevision},
        peripherals::LP_ANA,
        rtc_cntl::SocResetReason,
    };

    /// The detector this module enables, for logs.
    pub const DETECTOR: &str = "power glitch";

    /// `LP_ANA_FIB_ENABLE.ANA_FIB_ENA` bits 2..=5: 0 selects software control
    /// of the four voltage glitch reset enables (1: hardware default).
    const FIB_PWR_GLITCH: u32 = 0xF << 2;

    /// `LP_ANA_POWER_GLITCH_CNTL.PWR_GLITCH_RESET_ENA`: all four detectors.
    const PWR_GLITCH_RESET_ALL: u8 = 0xF;

    /// ESP-IDF v5.5 supports only rev v1.0 (ECO2) and later C5 silicon.
    const MIN_REVISION: ChipRevision = ChipRevision { major: 1, minor: 0 };

    /// Enable and verify the power glitch reset. Call once, early in the
    /// third-stage bootloader, after `esp_hal::init`.
    pub fn enable() -> Result<Report, Error> {
        let report = Report {
            reset_by_glitch: matches!(
                esp_hal::system::reset_reason(),
                Some(SocResetReason::PowerGlitch)
            ),
        };
        let revision = efuse::chip_revision();
        if revision < MIN_REVISION {
            return Err(Error::UnsupportedRevision(revision));
        }

        let ana = LP_ANA::regs();
        ana.fib_enable().modify(|r, w| {
            // SAFETY: only the four voltage glitch selections change; every
            // other analog FIB selection keeps its current value.
            unsafe {
                w.ana_fib_ena()
                    .bits(r.ana_fib_ena().bits() & !FIB_PWR_GLITCH)
            }
        });
        ana.ck_glitch_cntl().modify(|_, w| {
            // SAFETY: a 4-bit enable mask; every value is valid.
            unsafe { w.pwr_glitch_reset_ena().bits(PWR_GLITCH_RESET_ALL) }
        });

        if ana.fib_enable().read().ana_fib_ena().bits() & FIB_PWR_GLITCH != 0
            || ana.ck_glitch_cntl().read().pwr_glitch_reset_ena().bits() != PWR_GLITCH_RESET_ALL
        {
            return Err(Error::NotEnabled);
        }
        Ok(report)
    }
}
