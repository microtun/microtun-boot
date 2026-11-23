# ESP32-C5-DevKitC-1

microtun boot runs as a **third stage** on the ESP32-C5, as on the
[ESP32-C3](../esp32c3-supermini/README.md):

1. The mask ROM verifies the ESP-IDF v5.5.5 second stage at `0x2000`
   (Secure Boot V2, RSA-3072). The C5 ROM reserves `0x0..0x2000` for its key
   manager.
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

> **Status:** the ESP32-C5 port builds and its images, layout and boot chain
> pass the host-side checks, but the hardware-in-the-loop suite does not
> cover this board yet. Validate it on an expendable board before relying on
> it.

## Requirements and limitations

- ESP32-C5 **rev v1.0 (ECO2) or later**, the minimum ESP-IDF v5.5 supports.
  Both ESP32-C5-DevKitC-1 revisions (v1.1 with an N8R4 module, v1.2 with N8R8)
  have **8 MiB** of flash.
- Connect the board through the port labelled **ESP32-C5 USB** (the chip's
  native USB-Serial-JTAG), not the USB-to-UART port: the tools only flash and
  monitor over USB-Serial-JTAG, and microtun boot logs there.
- Secure Boot V2 is always enabled with the RSA-3072 scheme; the C5's ECDSA
  scheme is not used. There is no unsigned boot chain.
- Flash encryption and PSRAM are not supported by microtun boot, and ROM
  download mode stays enabled.
- Fault-injection hardening differs from the C3:
  - The C5 has no clock glitch detector. microtun boot re-asserts and checks
    the **power (voltage) glitch** reset ESP-IDF's second stage enables
    instead.
  - esp-hal has no TRNG entropy source for the C5, so the random delays in
    front of the hardened checks come from the hardware RNG **without** a
    guaranteed noise source. Treat the jitter as weaker than on the C3. See
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
| `image-0` (primary) | `0x6F000` | 3812 KiB |
| `image-1` (secondary) | `0x42F000` | 3812 KiB |
| `image-scratch` | `0x7E8000` | 64 KiB |
| `storage` (yours) | `0x7F8000` | 32 KiB |

microtun boot finds slots by label at runtime, so a changed partition table needs
no bootloader rebuild. Slots must start at `…F000` so the image after the
4 KiB MCUboot header lands on a 64 KiB MMU page. Check a layout with
`microtun-boot-tools-esp --board esp32c5-devkitc-1 layout`.

## Setup

You also need Docker with BuildKit to build the ESP-IDF second stage (v5.5.5,
the tag pinned in `bootloader/Dockerfile.boot`), or supply a prebuilt
unsigned one with `ESP_IDF_BOOTLOADER_BIN`. No `espflash` or `esptool` is
needed.

```sh
rustup target add riscv32imac-unknown-none-elf
python3 -m pip install imgtool
cargo install --path microtun-boot-esp/tools --locked
microtun-signer local init --key-id microtun-boot-local-p256 --algorithm p256
microtun-signer local init --key-id microtun-boot-local-esp32c5-sbv2-rsa3072 --algorithm rsa-pss
```

**Back up the local key store** (`microtun-signer local path`) before
provisioning a board: without the RSA key you cannot sign a new boot chain
for it.

Every `microtun-boot-tools-esp` command needs `--board esp32c5-devkitc-1` (or
`MICROTUN_BOOT_ESP_BOARD=esp32c5-devkitc-1`); the tool assumes no board. The
`cargo run` runners in this directory and in the
[blinky example](../../examples/esp32c5-devkitc-1-blinky/README.md) pass it for you.

## Build and sign without flashing

Set `NO_FLASH=1` on either command. The outputs land in
`target/riscv32imac-unknown-none-elf/release/` at the repository root:

| File | Flash address |
|---|---|
| `boot-chain.sbv2.bin` (second stage, partition table and microtun boot) | `0x2000` |
| `esp32c5-devkitc-1-firmware.signed.bin` | `image-0`, `0x6F000` |

The upgrade strategy is a build-time choice, selected by cargo features on
`esp32c5-devkitc-1-microtun-boot`: swap-using-scratch (default),
`overwrite-only`, `direct-xip` or `direct-xip-revert`.

## Flash a signed release binary

To flash an already signed boot chain, such as a release artifact, without
any signing key:

```sh
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-esp --board esp32c5-devkitc-1 \
  flash-bootloader --signed-bin \
  esp32c5-devkitc-1-release-boot-chain.sbv2.bin
```

The tool verifies the signatures and partition table first and writes the
image at `0x2000`. With `NO_FLASH=1` it only verifies.

## Writing an application

Start from the [blinky example](../../examples/esp32c5-devkitc-1-blinky/README.md),
not `boards/esp32c5-devkitc-1/firmware`: the board firmware contains
unauthenticated [test hooks](../../testing/microtun-boot-test-hooks/README.md) and
must never ship. An application must:

- Depend on [`microtun-boot-esp`](../../microtun-boot-esp/README.md) with the
  `esp32c5` feature (not `boot-runtime`), build for
  `riscv32imac-unknown-none-elf` and call
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
| `ESPPORT` | Serial port of the **ESP32-C5 USB** connector; auto-detected if only one ESP USB-Serial-JTAG device is attached |
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

The rollback floor lives in eFuse `BLOCK0.SECURE_VERSION` (bits 137–145),
which has only **9 epochs** on the ESP32-C5. Raising `SECURITY_COUNTER` and
booting that image permanently blocks older images, so reserve it for
security fixes. ESP-IDF's own app anti-rollback must stay disabled: microtun
boot owns the `SECURE_VERSION` eFuse and enforces it against the MCUboot
image header.

## Hardware-in-the-loop tests

[`microtun-boot-hil`](../../testing/microtun-boot-hil/README.md) does not support
this board yet. `boards/esp32c5-devkitc-1/firmware` already speaks the same
`@fbt` test-hook protocol as the ESP32-C3 firmware.
