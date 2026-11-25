<!-- microtun-boot-flashing-instructions -->
## Prerequisites

Install Rust/Cargo and Git. Make sure Cargo-installed binaries are on your `PATH` (normally via `~/.cargo/bin`). On Linux, installing the ESP tool also requires `libudev-dev` and `pkg-config`.

**ESP32-C3 SuperMini, ESP32-C5 Mini and ESP32-C6 SuperMini:**

```sh
cargo install --locked --git https://github.com/microtun/microtun-boot.git \
  --tag "@MICROTUN_BOOT_TAG@" microtun-boot-tools-esp
```

**RP2350 Pico 2:**

```sh
cargo install --locked --git https://github.com/microtun/microtun-boot.git \
  --tag "@MICROTUN_BOOT_TAG@" microtun-boot-tools-rp
```

You only need to install the tool for the board you want to flash.

## Flash the bootloaders

Download the attached `.bin` files. Choose `dev` or `release` to match the device's secure-boot key (**key provisioning is irreversible**).

**ESP32-C3 SuperMini** — Connect the board over USB-Serial-JTAG and set your serial port:

```sh
PROFILE=dev  # or release
export ESPPORT=/dev/ttyACM0   # change to your serial port
export MICROTUN_BOOT_OTP_BURN=YES
microtun-boot-tools-esp --board esp32c3-supermini flash-bootloader --signed-bin \
  "esp32c3-supermini-${PROFILE}-boot-chain.sbv2.bin"
```

The complete signed image (ESP-IDF second stage, partition table, and signed microtun boot) is verified and written at `0x0`. It does not re-sign or rebuild anything.

**ESP32-C5 Mini** — Connect the board through its USB-C connector (USB-Serial-JTAG) and set your serial port:

```sh
PROFILE=dev  # or release
export ESPPORT=/dev/ttyACM0   # change to your serial port
export MICROTUN_BOOT_OTP_BURN=YES
microtun-boot-tools-esp --board esp32c5-mini flash-bootloader --signed-bin \
  "esp32c5-mini-${PROFILE}-boot-chain.sbv2.bin"
```

The complete signed image is verified (including its partition table and flash size against the board) and written at `0x2000`, where the ESP32-C5 ROM loads the second stage. It does not re-sign or rebuild anything.

**ESP32-C6 SuperMini** — Connect the board through its USB-C connector (USB-Serial-JTAG) and set your serial port:

```sh
PROFILE=dev  # or release
export ESPPORT=/dev/ttyACM0   # change to your serial port
export MICROTUN_BOOT_OTP_BURN=YES
microtun-boot-tools-esp --board esp32c6-supermini flash-bootloader --signed-bin \
  "esp32c6-supermini-${PROFILE}-boot-chain.sbv2.bin"
```

The complete signed image is verified and written at `0x0`. It does not re-sign or rebuild anything.

**ESP32 chips in Secure Download Mode** (a manually burned `ENABLE_SECURITY_DOWNLOAD` eFuse; neither profile sets it): flash through a **USB-to-UART** connection instead of USB-Serial-JTAG, and set `ESPPORT` to it explicitly (for example `/dev/ttyUSB0`). None of the boards has an on-board bridge: the ESP32-C3 SuperMini needs an external adapter on GPIO20/GPIO21, the ESP32-C5 Mini one on GPIO11/GPIO12 (`TX`/`RX`), the ESP32-C6 SuperMini one on GPIO16/GPIO17. Over USB-Serial-JTAG, the RTC watchdog left armed by microtun boot can't be disabled in this mode and may reset the chip mid-write. See the board README's *Secure Download Mode* section.

**RP2350 Pico 2** — Connect an SWD probe:

```sh
PROFILE=dev  # or release
export MICROTUN_BOOT_OTP_BURN=YES
microtun-boot-tools-rp flash-bootloader --signed-bin \
  "rp2350-pico2-${PROFILE}-microtun-boot.signed.bin"
```

The ROM-sealed binary is verified and downloaded at `0x10000000`, then the board is reset, without generating a new signature. `PROBE_RS_PROBE`, `PROBE_RS_SPEED_KHZ`, and `PROBE_RS_CHIP` work as usual.
