# ESP32-C3 SuperMini blinky

A minimal Embassy application for the [microtun boot](../../README.md)
bootloader on an ESP32-C3 SuperMini, and a template for your own. It follows
the [shared example workflow](../README.md); this README has this board's
commands and details.

- Blinks the blue LED (GPIO8): **fast** while the image is an unconfirmed
  trial, **slow** once it is permanent.
- The **BOOT button** logs what microtun boot handed over: mode, slot, maximum
  image size and bootloader version.
- **Confirms** itself after a 5 s self-test. If it resets or panics before
  then, microtun boot reverts the upgrade.

## Prerequisites

The [shared prerequisites](../README.md#prerequisites), with this board's
target and tool:

```sh
rustup target add riscv32imc-unknown-none-elf
(cd ../.. && cargo install --path microtun-boot-esp/tools --locked)
```

## 1. Flash the dev boot chain (once per board)

> **Irreversible** — see the [shared warning](../README.md#1-flash-the-dev-boot-chain-once-per-board).

Download `esp32c3-supermini-dev-boot-chain.sbv2.bin` from a
[microtun boot release](https://github.com/microtun/microtun-boot/releases),
then flash the boot chain followed by the application:

```sh
export ESPPORT=/dev/ttyACM0
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-esp --board esp32c3-supermini \
  flash-bootloader --signed-bin \
  esp32c3-supermini-dev-boot-chain.sbv2.bin
cargo run --release      # sign, flash image-0 and open the monitor (Ctrl+C to exit)
```

## 2. Build, sign, flash

```sh
cargo run --release                          # IMAGE_VERSION defaults to 0.1.0
IMAGE_VERSION=0.2.0 cargo run --release
NO_FLASH=1 IMAGE_VERSION=0.2.0 cargo run --release   # sign only, e.g. for an OTA payload
# -> ../../target/riscv32imc-unknown-none-elf/release/esp32c3-supermini-blinky.signed.bin
```

What the output means and how a real upgrade is staged: see
[the shared walkthrough](../README.md#2-build-sign-flash).

## Use your own keys

Follow [use your own keys](../README.md#use-your-own-keys) with
`ESP_SBV2_KEY_ID=microtun-boot-local-esp32c3-sbv2-rsa3072`:

```sh
(cd boards/esp32c3-supermini/microtun-boot && NO_FLASH=1 cargo run --release)

cd examples/esp32c3-supermini-blinky
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-esp --board esp32c3-supermini \
  flash-bootloader --signed-bin \
  ../../target/riscv32imc-unknown-none-elf/release/boot-chain.sbv2.bin
cargo run --release
```

## Writing your own application

Copy this example as a template; see
[writing your own application](../README.md#writing-your-own-application)
and the [ESP32 board checklist](../../boards/README-ESP32.md#writing-an-application).
