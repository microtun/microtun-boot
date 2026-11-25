# RP2350 Pico 2 blinky

A minimal Embassy application for the [microtun boot](../../README.md)
bootloader on a Raspberry Pi Pico 2 or Pico 2 W, and a template for your own.
It follows the [shared example workflow](../README.md); this README has this
board's hardware, commands and details.

- Blinks the on-board LED: **fast** while the image is an unconfirmed trial,
  **slow** once it is permanent.
- Logs what microtun boot handed over at startup: mode, slot, maximum image size
  and bootloader version.
- **Confirms** itself after a 5 s self-test. If it resets before then,
  microtun boot reverts the upgrade.

## Hardware

- Pico 2 or Pico 2 W (RP2350A, 4 MiB flash).
- An SWD probe such as the Raspberry Pi Debug Probe. All flashing uses SWD,
  not BOOTSEL/UF2.
- For logs, a 3.3 V serial adapter on GPIO0 (Pico pin 1), 115200 8N1. The
  Debug Probe's UART port works and is used automatically.

**Pico 2 W:** the LED is driven by the radio, so build with
`--features pico2-w` (adds about 230 KiB of
[CYW43439 firmware](cyw43-firmware/README.md)). Do not use this feature on a
plain Pico 2.

## Prerequisites

The [shared prerequisites](../README.md#prerequisites), with this board's
target and tool:

```sh
rustup target add thumbv8m.main-none-eabihf
(cd ../.. && cargo install --path microtun-boot-rp/tools --locked)
```

## 1. Flash the dev bootloader (once per board)

> **Irreversible** — see the [shared warning](../README.md#1-flash-the-dev-boot-chain-once-per-board)
> and [Secure boot](../../boards/rp2350-pico2/README.md#secure-boot).

Download `rp2350-pico2-dev-microtun-boot.signed.bin` from a
[microtun boot release](https://github.com/microtun/microtun-boot/releases),
then flash the bootloader followed by the application:

```sh
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-rp flash-bootloader --signed-bin \
  rp2350-pico2-dev-microtun-boot.signed.bin
cargo run --release      # sign, flash image-0 and open the monitor (Ctrl+C to exit)
```

microtun boot only enrolls OTP once a valid application is installed, so
flashing the application last lets you watch that first boot. With several
probes attached, select one with `PROBE_RS_PROBE`; to use another serial port
for the monitor, set `RPPORT`.

## 2. Build, sign, flash

```sh
cargo run --release                          # IMAGE_VERSION defaults to 0.1.0
IMAGE_VERSION=0.2.0 cargo run --release
NO_FLASH=1 IMAGE_VERSION=0.2.0 cargo run --release   # sign only, e.g. for an OTA payload
# -> ../../target/thumbv8m.main-none-eabihf/release/rp2350-pico2-blinky.signed.bin
```

`cargo run` writes the primary slot directly, so the image boots as permanent
and blinks slowly:

```
[INFO] hello from image 0.1.0+0 in Primary (mode SwapScratch)
[INFO] image already permanent
[INFO] alive
```

A real upgrade is staged by the running application into `image-1` with the
[updater API](../../microtun-boot-core/README.md#updating-from-an-application),
followed by `mark_for_test()` and a reset. microtun boot then boots the new image
as a trial; it blinks fast for 5 s, then confirms itself:

```
[INFO] hello from image 0.2.0+0 in Primary (mode SwapScratch)
[WARN] trial image: self-testing for 5 s before confirming
[INFO] image 0.2.0+0 confirmed
```

A reset during those 5 s swaps back to the previous image. This example has
no update transport.

## Use your own keys

Follow [use your own keys](../README.md#use-your-own-keys) with
`RP2350_SECURE_BOOT_KEY_ID=microtun-boot-local-rp2350-rom-secp256k1`:

```sh
(cd boards/rp2350-pico2/microtun-boot && NO_FLASH=1 cargo run --release)

cd examples/rp2350-pico2-blinky
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-rp flash-bootloader --signed-bin \
  ../../target/thumbv8m.main-none-eabihf/release/rp2350-pico2-microtun-boot.signed.bin
cargo run --release
```

## Writing your own application

Copy this example and keep its `build.rs`, the `imagedef-none` and `--nmagic`
settings (the application is started by microtun boot, not the ROM, and must link
tightly into the window microtun boot maps), the watchdog feeding and the
confirm-after-self-test logic. The full checklist is in the
[board README](../../boards/rp2350-pico2/README.md#writing-an-application).
