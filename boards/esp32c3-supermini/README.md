# ESP32-C3 SuperMini

The ESP32-C3's Secure Boot V2 second stage is limited to 48 KiB, too small for
microtun boot, so microtun boot runs as a **third stage**:

1. The mask ROM verifies the ESP-IDF v5.5.5 second stage at `0x0`
   (Secure Boot V2, RSA-3072).
2. The second stage verifies microtun boot at `0x20000`, signed with the **same**
   RSA-3072 key.
3. microtun boot verifies the application (MCUboot format, ECDSA P-256) in
   `image-0` or `image-1` and starts it.

> **Flashing the boot chain is irreversible.** On first boot the second stage
> burns its signing key digest into eFuse and enables Secure Boot V2. From
> then on the chip only boots images signed with that key.

On any error microtun boot resets after two seconds and tries again, so a
persistent problem looks like a reboot loop. An image that fails verification
is never booted.

## Requirements and limitations

- ESP32-C3 **rev v0.3 (ECO3) or later** with **4 MiB** flash, connected
  through its native USB-Serial-JTAG port.
- Secure Boot V2 is always enabled; there is no unsigned boot chain.
- Flash encryption is not supported, and ROM download mode stays enabled.
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

microtun boot finds slots by label at runtime, so a changed partition table needs
no bootloader rebuild. Slots must start at `…F000` so the image after the
4 KiB MCUboot header lands on a 64 KiB MMU page. Check a layout with
`microtun-boot-tools-esp layout`.

## Setup

You also need Docker with BuildKit to build the ESP-IDF second stage (v5.5.5,
the tag pinned in `bootloader/Dockerfile.boot`), or supply a prebuilt
unsigned one with `ESP_IDF_BOOTLOADER_BIN`. No `espflash` or `esptool` is
needed.

```sh
rustup target add riscv32imc-unknown-none-elf
python3 -m pip install imgtool
cargo install --path microtun-boot-esp/tools --locked
microtun-signer local init --key-id microtun-boot-local-p256 --algorithm p256
microtun-signer local init --key-id microtun-boot-local-esp32c3-sbv2-rsa3072 --algorithm rsa-pss
```

**Back up the local key store** (`microtun-signer local path`) before
provisioning a board: without the RSA key you cannot sign a new boot chain
for it.

## Build and sign without flashing

Set `NO_FLASH=1` on either command. The outputs land in
`target/riscv32imc-unknown-none-elf/release/` at the repository root:

| File | Flash address |
|---|---|
| `boot-chain.sbv2.bin` (second stage, partition table and microtun boot) | `0x0` |
| `esp32c3-supermini-firmware.signed.bin` | `image-0`, `0x6F000` |

The upgrade strategy is a build-time choice, selected by cargo features on
`esp32c3-supermini-microtun-boot`: swap-using-scratch (default), `overwrite-only`,
`direct-xip` or `direct-xip-revert`.

## Flash a signed release binary

To flash an already signed boot chain, such as a release artifact, without
any signing key:

```sh
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-esp flash-bootloader --signed-bin \
  esp32c3-supermini-release-boot-chain.sbv2.bin
```

The tool verifies the signatures and partition table first. With `NO_FLASH=1`
it only verifies.

## Writing an application

Start from the [blinky example](../../examples/esp32c3-supermini-blinky/README.md),
not `boards/esp32c3-supermini/firmware`: the board firmware contains
unauthenticated [test hooks](../../testing/microtun-boot-test-hooks/README.md) and
must never ship. An application must:

- Depend on [`microtun-boot-esp`](../../microtun-boot-esp/README.md) with the
  `esp32c3` feature (not `boot-runtime`) and call
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
| `ESPPORT` | Serial port; auto-detected if only one ESP32-C3 is attached |
| `SLOT=image-1` | Write the secondary slot instead of the primary (see below) |
| `MICROTUN_BOOT_OTP_BURN=YES` | Required to flash the boot chain |
| `NO_FLASH=1` | Build and sign only |

**Signing** (see [SIGNING.md](../../SIGNING.md)):

| Variable | Purpose |
|---|---|
| `IMAGE_VERSION` | Application image version (default `0.0.0`) |
| `SECURITY_COUNTER` | Anti-rollback epoch of the image (default `0`; see below) |
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
[updater](../../microtun-boot-core/README.md#updating-from-an-application), not
by the flash tool.

For production, use a signer service with your own key IDs and set
`MICROTUN_BOOT_PUBKEYS` to your pinned application key.

## Security counter

The rollback floor lives in eFuse `BLOCK0.SECURE_VERSION`, which has only
**16 epochs**. Raising `SECURITY_COUNTER` and booting that image permanently
blocks older images, so reserve it for security fixes. ESP-IDF's own app
anti-rollback must stay disabled: it reads the factory app descriptor, while
microtun boot owns the `SECURE_VERSION` eFuse and enforces it against the MCUboot
image header.

## Hardware-in-the-loop tests

> Use a **dedicated, expendable board.** The tests sign with the development
> RSA key and, on a blank chip, enroll it permanently with no confirmation.

```sh
export MICROTUN_BOOT_ESP32C3_PORT=/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_YOUR_DEVICE_ID
cargo test -p microtun-boot-hil --features esp32c3-supermini,smoke -- --test-threads=1
```

The board must use the `dev` profile, because the tests read the security
counter over JTAG. See [`microtun-boot-hil`](../../testing/microtun-boot-hil/README.md).
