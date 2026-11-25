# ESP32-C3 SuperMini

microtun boot runs on the ESP32-C3 as a third stage after the ESP-IDF second
stage. The boot chain, setup, flashing, configuration and application
checklist shared with the other ESP32 boards are in
[README-ESP32.md](../README-ESP32.md); this README covers the C3 SuperMini
specifics.

The SuperMini is a generic board sold by several vendors; Maker Go's
[product page](https://makergo.cc/product/esp32-c3-supermini-development-board/)
and [wiki](https://wiki.icbbuy.com/doku.php?id=developmentboard:esp32-c3mini)
(schematic, pinout, getting started) describe one common variant.

## Requirements and limitations

- ESP32-C3 **rev v0.3 (ECO3) or later** with **4 MiB** flash, connected
  through its native USB-Serial-JTAG port.
- Only the clock glitch detector is used, and the ROM runs without it. See
  [fault-injection hardening](../../THREAT-MODEL.md#fault-injection-hardening).

## Flash layout

Defined in `layout/partitions.csv`:

| Region | Offset | Size |
|---|---|---|
| ESP-IDF second stage | `0x0` | ≤ 48 KiB |
| Partition table | `0x10000` | 4 KiB |
| `microtun-boot` | `0x20000` | 256 KiB |
| `image-0` (primary) | `0x6F000` | 1764 KiB |
| `image-1` (secondary) | `0x22F000` | 1764 KiB |
| `image-scratch` | `0x3E8000` | 64 KiB |
| `storage` (yours) | `0x3F8000` | 32 KiB |

## Setup

The shared [setup](../README-ESP32.md#setup) steps, with this board's values:

```sh
rustup target add riscv32imc-unknown-none-elf
microtun-signer local init --key-id microtun-boot-local-esp32c3-sbv2-rsa3072 --algorithm rsa-pss
```

## Security counter

The rollback floor lives in eFuse `BLOCK0.SECURE_VERSION`, which has only
**16 epochs**. See the shared [security counter](../README-ESP32.md#security-counter)
notes.

## Hardware-in-the-loop tests

**Use a dedicated, expendable board**; see the warning in the
[shared HIL section](../README-ESP32.md#hardware-in-the-loop-tests).

```sh
export MICROTUN_BOOT_ESP_PORT=/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_YOUR_DEVICE_ID
cargo test -p microtun-boot-hil --features esp32c3-supermini,smoke -- --test-threads=1
```

The board must use the `dev` profile, because the tests read the security
counter over JTAG. See [`microtun-boot-hil`](../../testing/microtun-boot-hil/README.md).
