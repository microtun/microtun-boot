//! Hardware-in-the-loop test suite for the microtun boot bootloaders.
//!
//! Physical boards run seven smoke checks (`tests/smoke.rs`); the
//! `smoke` feature selects these checks; `regression` selects the detailed
//! scenario matrices on QEMU. The opt-in `security-counter` feature selects
//! `tests/security_counter.rs`, which consumes one OTP/eFuse epoch on physical
//! boards or advances an isolated file-backed counter under QEMU.
//! Ordinary fixtures are signed using the actual chip eFuse/OTP floor (read
//! via JTAG/probe-rs on ESP32-C3/C6 or SWD on RP2350; not on ESP32-C5),
//! or the QEMU eFuse backing file.
//! Both suites build the
//! real bootloader and the example firmware, sign it with upstream
//! `imgtool`, compose a complete flash image of the board's size with
//! microtun-boot-core's own updater/trailer code, reset, and assert on the
//! console. Physical fixtures use a shared `microtun-boot-flash` worker (Rust
//! espflash for ESP32-C3/C5/C6 or probe-rs LZ4 restore for RP2350).
//! RP2350 bootloader variants are ROM-signed with the default development OTP policy.
//! The harness modules
//! below are board-independent; each board
//! module contributes a [`Board`] description plus its executable-format code
//! (ELF conversion, malformed-image mutations) and flash layout. Select the
//! board under test with `esp32c3-supermini`, `esp32c5-devkitc-1-n4`,
//! `esp32c5-devkitc-1-n8`, `esp32c5-devkitc-1-n16`, `esp32c6-supermini`,
//! `rp2350-pico2`, or `qemu`. The QEMU backend uses isolated guest processes
//! and persistent NOR files instead of a probe.
//!
//! Harness modules:
//! * [`artifacts`]: cargo builds of bootloader variants and the firmware.
//! * [`sign`]: imgtool signing plus MCUboot-envelope mutations.
//! * [`flash`]: whole-flash composer over microtun-boot-core.
//! * [`flash_transport`]: RP2350 single-pass LZ4 restore and probe reset.
//! * [`worker`]: the shared `microtun-boot-flash` worker binary's `main`.
//! * [`dut`]: ESP32 Rust espflash flashing, RP2350 SWD worker, and console capture.
//! * [`scenario`]: small helpers shared by the scenario tests.
//! * [`process`]: bounded subprocesses with live, libtest-captured output.
//!
//! Board modules: `esp32c3`, `esp32c5`, `esp32c6`, `rp2350` and `qemu` (selected by
//! feature). The ESP boards are descriptors over the shared `esp` back-end;
//! `esp32c5` describes the three ESP32-C5-DevKitC-1 flash variants.

pub mod artifacts;
mod board_lock;
pub mod dut;
pub mod flash;
#[cfg(feature = "rp2350-pico2")]
pub mod flash_transport;
pub mod process;
pub mod scenario;
pub mod sign;
pub mod worker;

#[cfg(feature = "qemu")]
pub mod qemu;

#[cfg(feature = "rp2350-pico2")]
pub type ProbeSession = probe_rs::Session;
#[cfg(not(feature = "rp2350-pico2"))]
pub type ProbeSession = ();

#[cfg(feature = "esp")]
pub mod esp;
#[cfg(feature = "esp32c3-supermini")]
pub mod esp32c3;
#[cfg(feature = "esp32c5")]
pub mod esp32c5;
#[cfg(feature = "esp32c6-supermini")]
pub mod esp32c6;
#[cfg(feature = "rp2350-pico2")]
pub mod rp2350;

pub use microtun_boot_test_hooks as hooks;

/// A concrete error for the host-side test harness. The harness surfaces
/// diagnostics from many tools, but does not need runtime error downcasting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// Convert errors from optional board-specific dependencies at their call site.
    pub fn external(error: impl std::fmt::Display) -> Self {
        Self(error.to_string())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self(message.into())
    }
}

macro_rules! from_external_error {
    ($($source:ty),* $(,)?) => {
        $(impl From<$source> for Error {
            fn from(error: $source) -> Self {
                Self::external(error)
            }
        })*
    };
}

from_external_error!(
    std::io::Error,
    std::env::VarError,
    std::num::ParseIntError,
    serialport::Error,
);

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// What the shared harness needs to know about one board. Each suite defines
/// a single static board description (lazily read from the table on ESP); composed [`flash::FlashImage`]s and
/// [`dut::Dut`]s carry a reference to it.
pub struct Board {
    /// Short name (`esp32c5-devkitc-1-n8`): the worker's board argument and
    /// the console prefix.
    pub name: &'static str,
    /// Directory under `boards/` (`esp32c5-devkitc-1`). It also names the
    /// build directory `target/hil-<dir>/`, the `boards/<dir>/microtun-boot`
    /// crate, `boards/<dir>/firmware` and their `<dir>-microtun-boot` and
    /// `<dir>-firmware` binaries. Boards that differ only in flash size
    /// share it, and with it their builds.
    pub dir: &'static str,
    /// Human-readable chip name for messages (`ESP32-C5`).
    pub chip: &'static str,
    /// Prefix of the suite's environment variables: `<prefix>_PORT`,
    /// ESP `<prefix>_FLASH_TIMEOUT_SECS`, or RP2350
    /// `<prefix>_PROBE_TIMEOUT_SECS` and `<prefix>_RESET_GATE`.
    pub env_prefix: &'static str,
    /// Rust target of the bootloader and firmware.
    pub target: &'static str,
    /// Capacity of the board's flash. Every fixture is a complete image of
    /// exactly this size.
    pub flash_size: usize,
    pub console: Console,
    /// Optional probe transport (RP2350 only).
    pub probe: Option<Probe>,
    pub signing: Signing,
}

/// Platform-specific native serial handle, so the reader owns a concrete type.
#[cfg(unix)]
pub type NativeSerialPort = serialport::TTYPort;
#[cfg(windows)]
pub type NativeSerialPort = serialport::COMPort;

/// The serial console the scenarios assert on.
pub struct Console {
    /// Completes "set `<prefix>_PORT` to …" when the variable is missing.
    pub port_hint: &'static str,
    /// Open the port; [`dut::open_serial`] is enough for an ordinary UART.
    pub open: fn(&str) -> serialport::Result<NativeSerialPort>,
    /// The console is the chip's own USB device, which a chip reset (panic,
    /// watchdog, ...) can make re-enumerate. The reader then reopens the port
    /// instead of treating a read error as the end of the console.
    pub reenumerates: bool,
}

/// RP2350 fixture transport and probe link.
pub struct Probe {
    /// Link name for messages (`SWD`).
    pub link: &'static str,
    /// Appended to every failed worker invocation.
    pub hint: &'static str,
    /// Fixture transport window for RP2350.
    pub flash_window: Option<FlashWindow>,
    /// Attach a debug session for fixture restoration.
    pub attach: fn() -> Result<ProbeSession>,
}

/// Virtual command-window address used by LZ4 fixture restore.
pub struct FlashWindow {
    pub window_address: u64,
}

/// imgtool parameters of the board's slots.
pub struct Signing {
    pub header_size: u32,
    pub slot_size: u32,
    pub sector: u32,
    /// Development VID/CID names, interpreted by imgtool as UUIDv5.
    pub vid: &'static str,
    pub cid: &'static str,
    /// Default `--rom-fixed` address, unless a test passes its own.
    pub rom_fixed: Option<u32>,
}

impl Board {
    /// `<env_prefix>_<suffix>`.
    pub fn env(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.env_prefix)
    }
}

/// A board for host-only unit tests: it has no console or probe.
#[cfg(test)]
pub(crate) static TEST_BOARD: Board = Board {
    name: "host",
    dir: "host",
    chip: "host",
    env_prefix: "MICROTUN_BOOT_HOST",
    target: "none",
    flash_size: flash::FLASH_SIZE,
    console: Console {
        port_hint: "nothing",
        open: |_| {
            Err(serialport::Error::new(
                serialport::ErrorKind::NoDevice,
                "test board",
            ))
        },
        reenumerates: false,
    },
    probe: None,
    signing: Signing {
        header_size: 0x1000,
        slot_size: 0x1000 * 32,
        sector: 0x1000,
        vid: "microtun.dev",
        cid: "host",
        rom_fixed: None,
    },
};

#[cfg(test)]
mod typed_harness_tests {
    use super::*;

    #[test]
    fn native_console_openers_return_concrete_handles() {
        let _: fn(&str) -> serialport::Result<NativeSerialPort> = dut::open_serial;
        let _: fn(&str) -> serialport::Result<NativeSerialPort> = TEST_BOARD.console.open;
    }

    #[test]
    fn concrete_errors_preserve_diagnostics() {
        let message: Error = "invalid fixture".into();
        assert_eq!(message.to_string(), "invalid fixture");
        let source = std::io::Error::other("console unavailable");
        let error: Error = source.into();
        assert_eq!(error.to_string(), "console unavailable");
    }
}
