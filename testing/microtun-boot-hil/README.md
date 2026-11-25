# microtun-boot-hil

End-to-end tests for the microtun boot boot chains. Each scenario builds the real
bootloader and test firmware, signs them, flashes a complete image, resets
the target and checks the console output.

> **Physical boards are expendable.** Every scenario reflashes the whole
> device, and the first test boot on a blank chip permanently enrolls the
> development secure-boot key, with no confirmation. Read the board README
> before connecting one.

## Running

Pick one target feature and one suite feature, from the repository root:

```sh
cargo test -p microtun-boot-hil --features qemu,regression
cargo test -p microtun-boot-hil --features esp32-c3-mini-1,smoke -- --test-threads=1
cargo test -p microtun-boot-hil --features esp32-c5-mini-1,smoke -- --test-threads=1
cargo test -p microtun-boot-hil --features esp32-c6-mini-1,smoke -- --test-threads=1
cargo test -p microtun-boot-hil --features rp2350-pico2,smoke -- --test-threads=1
```

| Target | Setup |
|---|---|
| `qemu` | [Patched QEMU](../../boards/qemu/README.md), no hardware |
| `esp32-c3-mini-1` | [ESP32-C3-MINI-1](../../boards/esp32-c3-mini-1/README.md#hardware-in-the-loop-tests) over USB-Serial-JTAG |
| `esp32-c5-mini-1` | [ESP32-C5-MINI-1](../../boards/esp32-c5-mini-1/README.md#hardware-in-the-loop-tests) over USB-Serial-JTAG |
| `esp32-c6-mini-1` | [ESP32-C6-MINI-1](../../boards/esp32-c6-mini-1/README.md#hardware-in-the-loop-tests) over USB-Serial-JTAG |
| `rp2350-pico2` | [Pico 2](../../boards/rp2350-pico2/README.md#hardware-in-the-loop-tests) over SWD and UART0 |

| Suite | Covers | Targets |
|---|---|---|
| `smoke` | Upgrades, trial confirmations, watchdog handoff and malformed images | Physical boards |
| `regression` | Full swap, overwrite, direct-XIP and safety matrices, plus `security-counter` | `qemu` |
| `security-counter` | Anti-rollback | QEMU, ESP32-C3, ESP32-C6, RP2350 |

Other combinations compile to no tests. Physical suites need
`--test-threads=1`.

**`security-counter` on a physical board permanently burns one eFuse/OTP
epoch per run**, and the ESP32-C3 and ESP32-C6 have only 16. ESP32-C5 hardware rollback
is disabled with esp-hal 1.2.2; its counter suite is excluded and its ordinary
fixtures use `SEC_CNT=0` without a JTAG fuse read. TODO(esp32c5): restore the
suite and DUT-floor signing when the eFuse implementation is verified.

All ESP boards share one back-end (`src/esp/`); `src/esp32c3/`,
`src/esp32c5/` and `src/esp32c6/` only describe the boards. The ESP HIL flash worker reuses the
ROM-only SPI attach, flash geometry, erase and data commands in
`microtun-boot-tools-esp::flash`: it sends no RAM stub or `FLASH_END`, and
leaves the reset to the parent after console capture starts. It retains the
ESP32-C5 reserved flash prefix and the worker timeout. After connecting, it
disables the RTC watchdog (which microtun boot leaves armed for 15 s across the
USB-Serial-JTAG reset) and sets the super watchdog to auto-feed, so the
fixture write (roughly 20 s for 4 MiB) isn't reset partway through. Because that needs
`WRITE_REG`, the ESP HIL suite does not support chips in Secure Download Mode.
Flash those through a USB-to-UART bridge as the board READMEs describe, and
keep a separate `dev`-profile board for HIL. The harness unit tests need no hardware:
`cargo test -p microtun-boot-hil --lib --features esp32-c3-mini-1,esp32-c5-mini-1,esp32-c6-mini-1`.

## Prerequisites

- `imgtool`, `microtun-signer` and the board's development keys
  (`microtun-signer local init`, see
  [SIGNING.md](../../SIGNING.md#development-keys-default)). Tests always
  sign with the local `microtun-boot-local-p256` key and pin the bootloaders'
  trusted keys to it, ignoring `MICROTUN_BOOT_KEY_ID` and `MICROTUN_BOOT_PUBKEYS`
  from your environment.
- The board's Rust target and tools, as listed in its README.

## Environment

| Variable | Purpose |
|---|---|
| `MICROTUN_BOOT_QEMU_BIN` | Patched `qemu-system-arm` (default: the one on `PATH`) |
| `MICROTUN_BOOT_ESP_PORT` | USB-Serial-JTAG port of the ESP board under test (ESP32-C3, ESP32-C5 or ESP32-C6) |
| `MICROTUN_BOOT_ESP_FLASH_TIMEOUT_SECS` | Limit for one complete ESP fixture write (default 600) |
| `MICROTUN_BOOT_RP235X_PORT` | RP2350 UART0 console |
| `RP2350_SECURE_BOOT_KEY_ID` | Local ROM key for a board enrolled with a non-development key |
| `PROBE_RS_PROBE`, `PROBE_RS_SPEED_KHZ` | Probe selection and SWD speed |
| `MICROTUN_BOOT_TEST_PROFILE`, `MICROTUN_BOOT_TEST_CARGO_ARGS` | Cargo profile (default `release`) and extra arguments for the board builds |

## Adding a scenario

Put it in the relevant `tests/*.rs` file, use the helpers in `tests/common/`
and `src/scenario.rs`, and register any new test binary in `Cargo.toml` with
its `required-features`. The test firmware is driven through its
[test hooks](../microtun-boot-test-hooks/README.md).

Under QEMU each boot gets a fresh virtual board. To keep one security counter
across reflashes, hold a `microtun_boot_hil::qemu::VirtualBoard` and boot through
it.
