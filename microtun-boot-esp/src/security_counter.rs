//! Application epoch in BLOCK0.SECURE_VERSION (repeat-coded bits, one per
//! epoch): 16 bits on the ESP32-C3 and ESP32-C6, 9 bits on the ESP32-C5.
//! USER_DATA is RS-coded and cannot be incrementally burned. IDF factory-app anti-rollback
//! must remain disabled: microtun boot owns this field exclusively.
//!
//! The ESP32-C6 shares the C3's field position, write-disable bit, register
//! names and controller timing (ESP-IDF v5.5.5 `efuse_hal.c` is identical for
//! both), so it uses the C3 branches.
//!
//! TODO(esp32c5): the C5 branches are retained for future work but this module
//! is compiled only on C3 and C6 for now (esp-hal 1.2.2 C5 eFuse issue). Before
//! restoring C5, verify refresh, error handling, write protection, monotonic
//! programming and read-back on an expendable device.
use core::sync::atomic::{AtomicBool, Ordering};

use esp_hal::{
    efuse::{self, SECURE_VERSION, WR_DIS},
    peripherals::EFUSE,
    time::{Duration, Instant},
};
use microtun_boot_core::{SecurityCounterError as E, SecurityCounterStore};

// SECURE_VERSION starts at BLOCK0 bit 142 (C3, C6) or 137 (C5): bit 14 or 9
// of programming word 4. All three chips share its write-disable bit.
#[cfg(any(feature = "esp32c3", feature = "esp32c6"))]
const VERSION_WORD_BIT: u32 = 14;
#[cfg(feature = "esp32c5")]
const VERSION_WORD_BIT: u32 = 9;
const VERSION_WRITE_DISABLE_BIT: u32 = 18;
// Each burnt bit is one epoch. The C5's field is 9 bits in ESP-IDF v5.5.5's
// eFuse table and esp-hal (its PAC's 16-bit error field covers reserved bits,
// which are never staged here).
#[cfg(any(feature = "esp32c3", feature = "esp32c6"))]
const SECURE_VERSION_BITS: u32 = 16;
#[cfg(feature = "esp32c5")]
const SECURE_VERSION_BITS: u32 = 9;
const SECURE_VERSION_MASK: u32 = (1 << SECURE_VERSION_BITS) - 1;
// Use the HAL monotonic timer, not a CPU-frequency-dependent spin count.
// This is deliberately generous relative to an eFuse read/program operation;
// a timeout aborts the current operation and the bootloader then resets with
// a delay. Do not reissue commands inside a timed-out eFuse transaction.
const EFUSE_OPERATION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug)]
enum Operation {
    Read,
    Program,
    Any,
}

/// Set by the first [`EfuseSecurityCounter::new`] of this boot.
///
/// The ESP32-C3 has no compare-and-swap, so this is a plain load then store
/// on every chip; that is race-free under `new`'s single-core,
/// interrupts-disabled contract.
static CREATED: AtomicBool = AtomicBool::new(false);

/// Bootloader-only eFuse access, before interrupts/app execution.
///
/// esp-hal does not hand out an `EFUSE` peripheral singleton: it reads the
/// eFuse read registers itself (chip revision, MAC address, ADC calibration)
/// and has no programming API. microtun boot is therefore the only code that
/// drives the programming registers, and at most one value of this type
/// exists per boot (checked at runtime).
pub struct EfuseSecurityCounter {
    _not_send_or_sync: core::marker::PhantomData<*const ()>,
}

impl EfuseSecurityCounter {
    /// # Panics
    /// If called more than once in the same boot.
    ///
    /// # Safety
    /// Not a memory-safety contract: `unsafe` marks the irreversible fuse
    /// burns this value performs. The caller guarantees:
    ///
    /// * `esp_hal::init` has run: the operation timeouts use its monotonic
    ///   timer, and would otherwise never expire;
    /// * interrupts are disabled and nothing else programs eFuses for this
    ///   value's lifetime (reads by esp-hal between operations are harmless);
    /// * SECURE_VERSION is reserved for microtun boot, and IDF APP_ANTI_ROLLBACK
    ///   is disabled, including in the second stage.
    pub unsafe fn new() -> Self {
        assert!(
            !CREATED.load(Ordering::Relaxed),
            "only one eFuse security counter may exist per boot"
        );
        CREATED.store(true, Ordering::Relaxed);
        Self {
            _not_send_or_sync: core::marker::PhantomData,
        }
    }

    fn clear_program_registers() {
        // Reset all data and RS staging registers to zero. No programming
        // command is issued here; only SECURE_VERSION is subsequently staged.
        let efuse = EFUSE::regs();
        #[cfg(any(feature = "esp32c3", feature = "esp32c6"))]
        {
            efuse.pgm_data0().reset();
            efuse.pgm_data1().reset();
            efuse.pgm_data2().reset();
            efuse.pgm_data3().reset();
            efuse.pgm_data4().reset();
            efuse.pgm_data5().reset();
            efuse.pgm_data6().reset();
            efuse.pgm_data7().reset();
            efuse.pgm_check_value0().reset();
            efuse.pgm_check_value1().reset();
            efuse.pgm_check_value2().reset();
        }
        #[cfg(feature = "esp32c5")]
        {
            for data in efuse.pgm_data_iter() {
                data.reset();
            }
            for check in efuse.pgm_check_value_iter() {
                check.reset();
            }
        }
    }

    /// Stage one SECURE_VERSION bit (`bit < SECURE_VERSION_BITS`) in
    /// programming word 4, after [`Self::clear_program_registers`].
    fn stage_version_bit(bit: u32) {
        debug_assert!(bit < SECURE_VERSION_BITS);
        let value = 1 << (VERSION_WORD_BIT + bit);
        #[cfg(any(feature = "esp32c3", feature = "esp32c6"))]
        EFUSE::regs().pgm_data4().write(|w| {
            // SAFETY: the caller checked `bit`, so only a SECURE_VERSION bit
            // (word-4 bits 14..=29) is staged.
            unsafe { w.pgm_data_4().bits(value) }
        });
        #[cfg(feature = "esp32c5")]
        EFUSE::regs().pgm_data(4).write(|w| {
            // SAFETY: the caller checked `bit`, so only a SECURE_VERSION bit
            // (word-4 bits 9..=17) is staged.
            unsafe { w.pgm_data().bits(value) }
        });
    }

    /// Repeat-coding errors in the counter bits. Error register 3 mirrors
    /// programming word 4 on every chip.
    fn repeat_errors() -> u32 {
        let efuse = EFUSE::regs();
        #[cfg(any(feature = "esp32c3", feature = "esp32c6"))]
        let errors = u32::from(efuse.rd_repeat_err3().read().secure_version_err().bits());
        #[cfg(feature = "esp32c5")]
        let errors = u32::from(
            efuse
                .rd_repeat_data_err3()
                .read()
                .secure_version_err()
                .bits(),
        );
        errors & SECURE_VERSION_MASK
    }

    #[cfg(feature = "esp32c5")]
    fn set_timing() {
        // Match ESP-IDF v5.5.5 components/hal/esp32c5/efuse_hal.c: the C5
        // eFuse controller needs no timing configuration.
    }

    #[cfg(any(feature = "esp32c3", feature = "esp32c6"))]
    fn set_timing() {
        // Match ESP-IDF v5.5.5 components/hal/esp32c3/efuse_hal.c (and the
        // identical esp32c6/efuse_hal.c).
        // The eFuse clock is fixed: these are not APB-frequency settings.
        // The ROM timing helper sets PWR_OFF_NUM to 0x40 instead of IDF's 0x190.
        let efuse = EFUSE::regs();
        // The PAC marks these fields `unsafe` only because the SVD carries no
        // value constraints; there is no safe setter for them.
        efuse.dac_conf().modify(|_, w| {
            // SAFETY: documented C3/C6 timing value; exclusive controller ownership.
            let w = unsafe { w.dac_num().bits(0xff) };
            // SAFETY: as above.
            unsafe { w.dac_clk_div().bits(0x28) }
        });
        // SAFETY: documented C3/C6 timing value (IDF efuse_hal.c).
        efuse
            .wr_tim_conf1()
            .modify(|_, w| unsafe { w.pwr_on_num().bits(0x3000) });
        // SAFETY: documented C3/C6 timing value (IDF efuse_hal.c).
        efuse
            .wr_tim_conf2()
            .modify(|_, w| unsafe { w.pwr_off_num().bits(0x190) });
    }

    fn wait_idle(operation: Operation, error: E) -> Result<(), E> {
        let started = Instant::now();
        loop {
            let command = EFUSE::regs().cmd().read();
            let busy = match operation {
                Operation::Read => command.read_cmd().bit_is_set(),
                Operation::Program => command.pgm_cmd().bit_is_set(),
                Operation::Any => command.read_cmd().bit_is_set() || command.pgm_cmd().bit_is_set(),
            };
            if !busy {
                return Ok(());
            }
            if started.elapsed() >= EFUSE_OPERATION_TIMEOUT {
                #[cfg(feature = "log")]
                log::error!(
                    "eFuse: timeout {:?}, read={}, program={}",
                    operation,
                    command.read_cmd().bit_is_set(),
                    command.pgm_cmd().bit_is_set()
                );
                return Err(error);
            }
            core::hint::spin_loop();
        }
    }

    fn refresh() -> Result<(), E> {
        #[cfg(feature = "log")]
        log::trace!("eFuse: read begin");
        // Never change timing or issue a command while another operation is busy.
        Self::wait_idle(Operation::Any, E::Busy)?;
        Self::set_timing();
        let efuse = EFUSE::regs();
        // SAFETY: documented read opcode (C3, C5, C6); no programming
        // command is issued.
        efuse
            .conf()
            .modify(|_, w| unsafe { w.op_code().bits(0x5aa5) });
        efuse.cmd().modify(|_, w| w.read_cmd().set_bit());
        Self::wait_idle(Operation::Read, E::Read)?;
        // Hardware workaround (C3, C5, C6, also present in ROM and IDF): check
        // READ_CMD again to ensure the eFuse clock is normal before accessing
        // read registers.
        Self::wait_idle(Operation::Read, E::Read)?;
        #[cfg(feature = "log")]
        log::trace!("eFuse: read complete");
        Ok(())
    }
}

impl SecurityCounterStore for EfuseSecurityCounter {
    fn capacity(&self) -> u32 {
        SECURE_VERSION_BITS
    }

    fn get(&mut self) -> Result<u32, E> {
        // Refresh physical fuses; do not trust app-writable shadow registers.
        Self::refresh()?;
        // Reject repeat-coding errors in the counter, rather than interpreting
        // a damaged fuse as a smaller floor.
        if Self::repeat_errors() != 0 {
            return Err(E::InvalidState);
        }
        let bits = efuse::read_field_le::<u16>(SECURE_VERSION);
        let value = bits.count_ones();
        #[cfg(feature = "log")]
        log::debug!("eFuse: floor={}", value);
        Ok(value)
    }

    fn advance(&mut self, value: u32) -> Result<(), E> {
        if value > self.capacity() {
            return Err(E::Exhausted);
        }
        let current = self.get()?;
        if current >= value {
            return Ok(());
        }
        if efuse::read_field_le::<u32>(WR_DIS) & (1 << VERSION_WRITE_DISABLE_BIT) != 0 {
            return Err(E::Locked);
        }
        // Count burnt bits (same encoding as IDF SECURE_VERSION). Burn exactly
        // one fresh bit at a time, preserving pre-existing non-prefix values.
        for _ in current..value {
            let old = efuse::read_field_le::<u16>(SECURE_VERSION);
            let bit = (!old).trailing_zeros();
            // `current < value <= SECURE_VERSION_BITS` implies a clear bit
            // inside the field, but check it here: a glitched or inconsistent
            // read must never stage a bit outside SECURE_VERSION.
            if bit >= SECURE_VERSION_BITS {
                return Err(E::InvalidState);
            }
            // Exclusive boot-time controller access, BLOCK0 only; clear ALL
            // staging registers before and after programming so no unrelated
            // secure-boot, key or write-disable fuse can be burned.
            Self::wait_idle(Operation::Any, E::Busy)?;
            Self::clear_program_registers();
            Self::stage_version_bit(bit);
            let efuse = EFUSE::regs();
            Self::set_timing();
            // SAFETY: documented programming opcode (C3, C5, C6), with only
            // BLOCK0 staged.
            efuse
                .conf()
                .modify(|_, w| unsafe { w.op_code().bits(0x5a5a) });
            #[cfg(feature = "log")]
            log::debug!("eFuse: program BLOCK0 bit={} begin", bit);
            // SAFETY: block 0 is the only target; write clears READ_CMD and
            // explicitly selects BLOCK0 before issuing the programming command.
            efuse
                .cmd()
                .write(|w| unsafe { w.blk_num().bits(0).pgm_cmd().set_bit() });
            // On timeout do not clear staging or issue another operation.
            Self::wait_idle(Operation::Program, E::Write)?;
            Self::clear_program_registers();
            #[cfg(feature = "log")]
            log::debug!("eFuse: program complete; verifying");
            if self.get()? != old.count_ones() + 1 {
                return Err(E::NotPersisted);
            }
        }
        Ok(())
    }
}
