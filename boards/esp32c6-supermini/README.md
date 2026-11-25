# ESP32-C6 SuperMini

microtun boot runs on the ESP32-C6 as a third stage after the ESP-IDF second
stage. The boot chain, setup, flashing, configuration and application
checklist shared with the other ESP32 boards are in
[README-ESP32.md](../README-ESP32.md); this README covers the C6 SuperMini
specifics.

The SuperMini is a generic board sold by several vendors; Maker Go's
[product page](https://makergo.cc/product/esp32-c6-supermini-development-board/)
and [wiki](https://wiki.icbbuy.com/doku.php?id=developmentboard:esp32-c6mini)
(schematic) describe one common variant.

> **Status:** the ESP32-C6 port builds and its images, layout and boot chain
> pass the host-side checks. The RAM handoff's C6 register sequence and the
> eFuse security counter have not yet been run on hardware: run the
> [hardware-in-the-loop suite](#hardware-in-the-loop-tests) on an expendable
> board before relying on the port.

## Requirements and limitations

- An ESP32-C6 SuperMini (ESP32-C6FH4, **4 MiB** in-package flash), any chip
  revision, connected through its USB-C port (the chip's native
  USB-Serial-JTAG on GPIO12/GPIO13). "SuperMini" boards come from several
  vendors; one with a different flash size needs its own partition table.
- **No glitch detector.** ESP-IDF supports no clock or power glitch reset on
  the C6, so microtun boot relies on its fault-hardened, TRNG-jittered checks
  alone. See
  [fault-injection hardening](../../THREAT-MODEL.md#fault-injection-hardening).
- The bootloader is built with esp-hal's `enable-pmp` option **off**
  (`ESP_HAL_CONFIG_ENABLE_PMP=false` in `.cargo/config.toml`): esp-hal would
  otherwise lock PMP regions over microtun boot's RAM, and the locked regions
  would stop the RAM handoff from loading the application. The build fails if
  the option is on. Applications keep esp-hal's default.
- The C6's flash MMU page size is configurable. microtun boot's RAM handoff
  always selects 64 KiB pages, esp-hal's default; keep
  `ESP_BOOTLOADER_ESP_IDF_CONFIG_MMU_PAGE_SIZE` at `64k` in applications.

## Flash layout

Defined in `layout/partitions.csv`:

| Region | Offset | Size |
|---|---|---|
| ESP-IDF second stage | `0x0` | ≤ 64 KiB |
| Partition table | `0x10000` | 4 KiB |
| `microtun-boot` | `0x20000` | 256 KiB |
| `image-0` (primary) | `0x6F000` | 1764 KiB |
| `image-1` (secondary) | `0x22F000` | 1764 KiB |
| `image-scratch` | `0x3E8000` | 64 KiB |
| `storage` (yours) | `0x3F8000` | 32 KiB |

## Setup

The shared [setup](../README-ESP32.md#setup) steps, with this board's values:

```sh
rustup target add riscv32imac-unknown-none-elf
microtun-signer local init --key-id microtun-boot-local-esp32c6-sbv2-rsa3072 --algorithm rsa-pss
```

## Security counter

The rollback floor lives in eFuse `BLOCK0.SECURE_VERSION` (bits 142–157, as
on the ESP32-C3), which has only **16 epochs**. See the shared
[security counter](../README-ESP32.md#security-counter) notes.

## Hardware-in-the-loop tests

**Use a dedicated, expendable board**; see the warning in the
[shared HIL section](../README-ESP32.md#hardware-in-the-loop-tests).

```sh
export MICROTUN_BOOT_ESP_PORT=/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_YOUR_DEVICE_ID
cargo test -p microtun-boot-hil --features esp32c6-supermini,smoke -- --test-threads=1
```

The board must use the `dev` profile, because the tests read the security
counter over JTAG. The `security-counter` suite burns one of the 16 epochs
per run. See [`microtun-boot-hil`](../../testing/microtun-boot-hil/README.md).
