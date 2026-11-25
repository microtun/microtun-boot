# ESP32-C5-MINI-1

microtun boot runs on the ESP32-C5 as a third stage after the ESP-IDF second
stage. The boot chain, setup, flashing, configuration and application
checklist shared with the other ESP32 boards are in
[README-ESP32.md](../README-ESP32.md); this README covers the
ESP32-C5-MINI-1 specifics.

> **Status:** the ESP32-C5 port builds and its images, layout and boot chain
> pass the host-side checks. The
> [hardware-in-the-loop suite](#hardware-in-the-loop-tests) supports this
> board; run it on an expendable board before relying on the port.

## Requirements and limitations

- An ESP32-C5-MINI-1-N4 or -1U-N4 (ESP32-C5HF4, **4 MiB** in-package
  flash, no PSRAM) with chip **rev v1.0 (ECO2) or later**, the minimum
  ESP-IDF v5.5 supports. Connect it through the chip's native
  USB-Serial-JTAG (GPIO13/GPIO14): the tools auto-detect and monitor over
  USB-Serial-JTAG, and microtun boot logs there.
- Fault-injection hardening differs from the C3:
  - The C5 has no clock glitch detector. microtun boot re-asserts and checks
    the **power (voltage) glitch** reset ESP-IDF's second stage enables
    instead.
  - esp-hal 1.2.2 has no TRNG entropy source for the C5. Random delays are
    disabled; the hardened signature and load-plan checks still run, without
    jitter. TODO(esp32c5): restore jitter once a real TRNG source is supported
    and verified. See
    [fault-injection hardening](../../THREAT-MODEL.md#fault-injection-hardening).
- The bootloader is built with esp-hal's `enable-pmp` option **off**
  (`ESP_HAL_CONFIG_ENABLE_PMP=false` in `.cargo/config.toml`): esp-hal would
  otherwise lock PMP regions over microtun boot's RAM, and the locked regions
  would stop the RAM handoff from loading the application. The build fails if
  the option is on. Applications keep esp-hal's default.

## Flash layout

Defined in `layout/partitions.csv`:

| Region | Offset | Size |
|---|---|---|
| Reserved (key manager) | `0x0` | 8 KiB |
| ESP-IDF second stage | `0x2000` | ≤ 56 KiB |
| Partition table | `0x10000` | 4 KiB |
| `microtun-boot` | `0x20000` | 256 KiB |
| `image-0` (primary) | `0x6F000` | 1764 KiB |
| `image-1` (secondary) | `0x22F000` | 1764 KiB |
| `image-scratch` | `0x3E8000` | 64 KiB |
| `storage` (yours) | `0x3F8000` | 32 KiB |

The C5 ROM reserves `0x0..0x2000` for its key manager; nothing else may use
it.

## Setup

The shared [setup](../README-ESP32.md#setup) steps, with this board's values:

```sh
rustup target add riscv32imac-unknown-none-elf
microtun-signer local init --key-id microtun-boot-local-esp32c5-sbv2-rsa3072 --algorithm rsa-pss
```

A prebuilt second stage supplied through `ESP_IDF_BOOTLOADER_BIN` must be
built for 4 MiB of flash.

## Security counter

**Temporarily disabled on ESP32-C5** due to the eFuse issue with esp-hal
1.2.2. microtun boot does not read or burn a hardware rollback floor, require
`SEC_CNT`, or report a floor in boot information. Signing still includes
`SECURITY_COUNTER` by default for image-format compatibility; increasing it
has no rollback effect. The ordinary version-based downgrade-prevention
feature remains enabled by default, but is not hardware anti-rollback.
Older signed images may be booted after a flash rewrite. Do not rely on this
board for hardware rollback resistance.

TODO(esp32c5): revisit when the HAL/eFuse issue is fixed. Reserve eFuse
`BLOCK0.SECURE_VERSION` (bits 137–145, **9 epochs**) for future support.
Keep ESP-IDF's own app anti-rollback disabled. On expendable hardware verify
reads, burns, read-back, power-loss recovery and rejection of older epochs;
then restore the policy requirement, store, final handoff check, boot-info
floor and HIL suite. Restore TRNG jitter once a real entropy source is
available and tested.

## Hardware-in-the-loop tests

**Use a dedicated, expendable board**; see the warning in the
[shared HIL section](../README-ESP32.md#hardware-in-the-loop-tests).

```sh
export MICROTUN_BOOT_ESP_PORT=/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_YOUR_DEVICE_ID
cargo test -p microtun-boot-hil --features esp32-c5-mini-1,smoke -- --test-threads=1
```

Each test composes a complete 4 MiB fixture and writes from `0x2000` to the
end of flash (4,186,112 bytes) through the ROM loader. The key-manager
sectors below `0x2000` are left alone. Raise
`MICROTUN_BOOT_ESP_FLASH_TIMEOUT_SECS` (default 600) if a write times out.
The second stage is built with Docker as for a normal build, or taken from
`ESP_IDF_BOOTLOADER_BIN` (built for 4 MiB of flash).

Use the `dev` profile for the HIL setup. C5 smoke fixtures use counter `0`
without reading eFuses over JTAG. The `security-counter` suite is temporarily
excluded on C5 and will not consume any `SECURE_VERSION` epochs. See
[`microtun-boot-hil`](../../testing/microtun-boot-hil/README.md).
