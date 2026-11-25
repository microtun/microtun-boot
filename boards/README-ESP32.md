# ESP32 boards: shared boot chain and workflow

Everything in this file applies equally to the three ESP32 boards:
[ESP32-C3-MINI-1](esp32-c3-mini-1/README.md),
[ESP32-C5-MINI-1](esp32-c5-mini-1/README.md) and
[ESP32-C6-MINI-1](esp32-c6-mini-1/README.md). Each board README covers
only what is specific to that board: requirements, flash layout, chip
quirks, its security counter and its status.

## The boot chain

The ESP-IDF Secure Boot V2 second stage is limited to 48 KiB (C3), 56 KiB (C5)
or 64 KiB (C6) — too small for microtun boot — so microtun boot runs as a
**third stage**:

1. The mask ROM verifies the ESP-IDF v5.5.5 second stage (Secure Boot V2,
   RSA-3072) at the second-stage offset of the board's flash layout.
2. The second stage verifies microtun boot at `0x20000`, signed with the
   **same** RSA-3072 key.
3. microtun boot verifies the application (MCUboot format, ECDSA P-256) in
   `image-0` or `image-1` and starts it.

> **Flashing the boot chain is irreversible.** On first boot the second stage
> burns its signing key digest into eFuse and enables Secure Boot V2. From
> then on the chip only boots images signed with that key.

On any error microtun boot resets after two seconds and tries again, so a
persistent problem looks like a reboot loop. An image that fails verification
is never booted.

Common to all three boards:

- Secure Boot V2 is always enabled with the RSA-3072 scheme; the C5's and C6's
  ECDSA schemes are not used. There is no unsigned boot chain.
- Flash encryption is not supported, and ROM download mode stays enabled.
- A chip in **Secure Download Mode** must be flashed through a USB-to-UART
  adapter instead; see [Secure Download Mode](#secure-download-mode).

## Flash layouts

Each board's layout is defined in its `layout/partitions.csv`. microtun boot
finds slots by label at runtime, so a changed partition table needs no
bootloader rebuild. Slots must start at `…F000` so the image after the
4 KiB MCUboot header lands on a 64 KiB MMU page. Check a layout with
`microtun-boot-tools-esp --board <board> layout`.

## Setup

Besides the [repository requirements](../README.md#requirements), building
the ESP-IDF second stage needs Docker with BuildKit (v5.5.5, the tag pinned
in each board's `bootloader/Dockerfile.boot`), or a prebuilt unsigned one
supplied through `ESP_IDF_BOOTLOADER_BIN`. No `espflash` or `esptool` is
needed.

Each board README lists this board's Rust target and RSA key ID; the rest is
the same for all boards:

```sh
rustup target add <target>                       # from the board README
python3 -m pip install imgtool
cargo install --path microtun-boot-esp/tools --locked
microtun-signer local init --key-id microtun-boot-local-p256 --algorithm p256
microtun-signer local init --key-id <esp32cX-sbv2-rsa3072> --algorithm rsa-pss   # from the board README
```

**Back up the local key store** (`microtun-signer local path`) before
provisioning a board: without the RSA key you cannot sign a new boot chain
for it.

Every `microtun-boot-tools-esp` command needs `--board <board>` (or
`MICROTUN_BOOT_ESP_BOARD=<board>`); the tool assumes no board. The
`cargo run` runners in the board directories and in the
[blinky examples](../examples/README.md) pass it for you.

## Build and sign without flashing

Set `NO_FLASH=1` on either command. The outputs land in
`target/<target>/release/` at the repository root:

| File | Flash address |
|---|---|
| `boot-chain.sbv2.bin` (second stage, partition table and microtun boot) | the second-stage offset of the board's flash layout |
| `<board>-firmware.signed.bin` | `image-0`, `0x6F000` |

The upgrade strategy is a build-time choice, selected by cargo features on
`<board>-microtun-boot`: swap-using-scratch (default), `overwrite-only`,
`direct-xip` or `direct-xip-revert`.

## Flash a signed release binary

To flash an already signed boot chain, such as a release artifact, without
any signing key:

```sh
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-esp --board <board> \
  flash-bootloader --signed-bin <board>-release-boot-chain.sbv2.bin
```

The tool verifies the signatures and the partition table (on the C5 also the
flash size in both stages' headers) against the board first, and writes the
image at the board's second-stage offset. With `NO_FLASH=1` it only verifies.

## Writing an application

Start from a [blinky example](../examples/README.md), not
`boards/<board>/firmware`: the board firmware contains unauthenticated
[test hooks](../testing/microtun-boot-test-hooks/README.md) and must never
ship. An application must:

- Depend on [`microtun-boot-esp`](../microtun-boot-esp/README.md) with the
  board's chip feature (`esp32c3`, `esp32c5` or `esp32c6`; not
  `boot-runtime`), build for the board's Rust target, and call
  `microtun_boot_build::configure_esp_application()` in `build.rs`.
- `esp_hal::init` disables the watchdog microtun boot leaves running, so re-arm
  one immediately after it, for example with
  `microtun_boot_esp::watchdog::start_rtc`.
- Confirm a trial upgrade only after its own health checks. A reset before
  confirmation reverts the upgrade.

## Configuration

**Flashing:**

| Variable | Purpose |
|---|---|
| `MICROTUN_BOOT_ESP_BOARD` | Board for `microtun-boot-tools-esp` when `--board` is absent |
| `ESPPORT` | Serial port; auto-detected if only one ESP USB-Serial-JTAG device is attached. In Secure Download Mode, set it to a USB-to-UART adapter instead (never auto-detected) |
| `SLOT=image-1` | Write the secondary slot instead of the primary (see below) |
| `MICROTUN_BOOT_OTP_BURN=YES` | Required to flash the boot chain |
| `NO_FLASH=1` | Build and sign only |

**Signing** (see [SIGNING.md](../SIGNING.md)):

| Variable | Purpose |
|---|---|
| `IMAGE_VERSION` | Application image version (default `0.0.0`) |
| `SECURITY_COUNTER` | Anti-rollback epoch of the image (default `0`; see the board's [security counter](#security-counter)) |
| `MICROTUN_SIGNER_URL` | `local:` (default) or a signer service |
| `MICROTUN_BOOT_KEY_ID`, `ESP_SBV2_KEY_ID` | Application (P-256) and boot-chain (RSA-3072) key IDs |
| `MICROTUN_BOOT_PUBKEYS` | Application public keys trusted by microtun boot (default: the local application signing key) |

**Boot-chain build:**

| Variable | Purpose |
|---|---|
| `ESP_BOOTLOADER_PROFILE` | `dev` (default; JTAG and spare key slots stay open) or `release` (disables JTAG, revokes spare slots) |
| `ESP_IDF_BOOTLOADER_BIN` | Prebuilt unsigned second stage; skips Docker |

`SLOT=image-1` only places an image in the secondary slot; it does not request
an upgrade. The trailer that triggers one is written by the running
application through the
[updater](../microtun-boot-core/README.md#updating-from-an-application), not
by the flash tool.

For production, use a signer service with your own key IDs and set
`MICROTUN_BOOT_PUBKEYS` to your pinned application key.

## Secure Download Mode

Neither bootloader profile enables Secure Download Mode, but a chip may have
it from a manually burned `ENABLE_SECURITY_DOWNLOAD` eFuse (for example, at
manufacture). On such a chip, **flash through a USB-to-UART adapter**, not the
native USB-Serial-JTAG port.

The reason is the RTC (LP) watchdog. microtun boot arms it for 15 s, and a
USB-Serial-JTAG reset into the ROM downloader leaves it running. Normally the
flash tool turns it off with `WRITE_REG` after connecting. Secure Download
Mode rejects `WRITE_REG`, so the tool can only warn, and any write longer than
the remaining watchdog time is cut off by a chip reset. A reset through `EN`
also resets the watchdog.

The modules have no USB-to-UART bridge, so use an external 3.3 V adapter on
UART0, plus GND:

| Module | Adapter TX → | Adapter RX → | BOOT strapping pin |
|---|---|---|---|
| ESP32-C3-MINI-1 | GPIO20 (U0RXD) | GPIO21 (U0TXD) | GPIO9 |
| ESP32-C5-MINI-1 | GPIO12 (U0RXD) | GPIO11 (U0TXD) | GPIO28 |
| ESP32-C6-MINI-1 | GPIO17 (U0RXD) | GPIO16 (U0TXD) | GPIO9 |

Unless the adapter's DTR/RTS drive `EN` and the BOOT pin, enter download mode
by hand: hold **BOOT** low, pulse **EN** (RESET) low, then release **BOOT**.

```sh
export ESPPORT=/dev/ttyUSB0   # the adapter; it is never auto-detected
```

The UART runs at 115200 baud, so large writes are much slower than over
USB-Serial-JTAG. On the ESP32-C5-MINI-1, microtun boot still logs on
USB-Serial-JTAG, so keep it connected if you want to watch the boot.

## Security counter

Raising `SECURITY_COUNTER` and booting that image permanently blocks older
images, so reserve it for security fixes. ESP-IDF's own app anti-rollback
must stay disabled: it reads the factory app descriptor, while microtun boot
owns the `SECURE_VERSION` eFuse and enforces it against the MCUboot image
header.

| Board | eFuse | Epochs | Status |
|---|---|---|---|
| [ESP32-C3-MINI-1](esp32-c3-mini-1/README.md#security-counter) | `BLOCK0.SECURE_VERSION` | 16 | Enforced |
| [ESP32-C5-MINI-1](esp32-c5-mini-1/README.md#security-counter) | — | — | Temporarily disabled; see the board README |
| [ESP32-C6-MINI-1](esp32-c6-mini-1/README.md#security-counter) | `BLOCK0.SECURE_VERSION` (bits 142–157) | 16 | Enforced |

## Hardware-in-the-loop tests

> Use a **dedicated, expendable board.** The tests sign with the development
> RSA key and, on a blank chip, enroll it permanently with no confirmation.

Each board README has the exact command and the board-specific notes. Boards
must use the `dev` bootloader profile: the `release` profile disables JTAG and
revokes spare key slots. The suites themselves are described in
[`microtun-boot-hil`](../testing/microtun-boot-hil/README.md).

## Compatible development boards

The board targets are named after the Espressif module each one matches.
Maker Go's small development boards carry the same chip, flash size and
USB-Serial-JTAG wiring, so they use the matching target unchanged:

| Development board | Target | Notes |
|---|---|---|
| [ESP32-C3 SuperMini](https://makergo.cc/product/esp32-c3-supermini-development-board/) ([wiki](https://wiki.icbbuy.com/doku.php?id=developmentboard:esp32-c3mini)) | `esp32-c3-mini-1` | Blue LED on GPIO8 (active low), BOOT button on GPIO9 |
| NiceMCU [ESP32-C5 Mini](https://wiki.icbbuy.com/doku.php?id=developmentboard:esp32-c5mini) | `esp32-c5-mini-1` | User LED on GPIO23, BOOT button on GPIO28; UART0 on the header's `TX`/`RX` |
| [ESP32-C6 SuperMini](https://makergo.cc/product/esp32-c6-supermini-development-board/) ([wiki](https://wiki.icbbuy.com/doku.php?id=developmentboard:esp32-c6mini)) | `esp32-c6-mini-1` | WS2812 RGB LED on GPIO8, BOOT button on GPIO9 |

The ESP32 [blinky examples](../examples/README.md) are written for these
boards and use their LED and button pins. "SuperMini" boards come
from several vendors; check that a clone has the same chip and 4 MiB of
flash before using the target.
