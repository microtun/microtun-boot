# ESP32-C5 Mini

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
> pass the host-side checks. The
> [hardware-in-the-loop suite](#hardware-in-the-loop-tests) supports this
> board; run it on an expendable board before relying on the port.

## Requirements and limitations

- An ESP32-C5 Mini (ESP32-C5HF4, **4 MiB** in-package flash, no
  PSRAM) with chip **rev v1.0 (ECO2) or later**, the minimum ESP-IDF v5.5
  supports. Connect it through its USB-C port (the chip's native
  USB-Serial-JTAG on GPIO13/GPIO14): the tools auto-detect and monitor over
  USB-Serial-JTAG, and microtun boot logs there. A chip in **Secure Download
  Mode** must be flashed through a USB-to-UART adapter instead; see
  [Secure Download Mode](#secure-download-mode).
- Secure Boot V2 is always enabled with the RSA-3072 scheme; the C5's ECDSA
  scheme is not used. There is no unsigned boot chain.
- Flash encryption is not supported, and ROM download mode stays enabled.
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

microtun boot finds slots by label at runtime, so a changed partition table needs
no bootloader rebuild. Slots must start at `…F000` so the image after the
4 KiB MCUboot header lands on a 64 KiB MMU page. Check a layout with
`microtun-boot-tools-esp --board esp32c5-mini layout`.

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

Every `microtun-boot-tools-esp` command needs `--board esp32c5-mini` (or
`MICROTUN_BOOT_ESP_BOARD=esp32c5-mini`); the tool assumes no board. The
`cargo run` runners in this directory and in the
[blinky example](../../examples/esp32c5-mini-blinky/README.md) pass it for you.

## Build and sign without flashing

Set `NO_FLASH=1` on either command. The outputs land in
`target/riscv32imac-unknown-none-elf/release/` at the repository root:

| File | Flash address |
|---|---|
| `boot-chain.sbv2.bin` (second stage, partition table and microtun boot) | `0x2000` |
| `esp32c5-mini-firmware.signed.bin` | `image-0`, `0x6F000` |

The upgrade strategy is a build-time choice, selected by cargo features on
`esp32c5-mini-microtun-boot`: swap-using-scratch (default),
`overwrite-only`, `direct-xip` or `direct-xip-revert`.

## Flash a signed release binary

To flash an already signed boot chain, such as a release artifact, without
any signing key:

```sh
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-esp --board esp32c5-mini \
  flash-bootloader --signed-bin \
  esp32c5-mini-release-boot-chain.sbv2.bin
```

The tool verifies the signatures, the partition table and the flash size in
both stages' headers against the board first, and writes the image at
`0x2000`. With `NO_FLASH=1`
it only verifies.

## Writing an application

Start from the [blinky example](../../examples/esp32c5-mini-blinky/README.md),
not `boards/esp32c5-mini/firmware`: the board firmware contains
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
| `ESPPORT` | Serial port; auto-detected if only one ESP USB-Serial-JTAG device is attached. In Secure Download Mode, set it to a USB-to-UART adapter instead (never auto-detected) |
| `SLOT=image-1` | Write the secondary slot instead of the primary (see below) |
| `MICROTUN_BOOT_OTP_BURN=YES` | Required to flash the boot chain |
| `NO_FLASH=1` | Build and sign only |

**Signing** (see [SIGNING.md](../../SIGNING.md)):

| Variable | Purpose |
|---|---|
| `IMAGE_VERSION` | Application image version (default `0.0.0`) |
| `SECURITY_COUNTER` | Signed metadata only on C5 (default `0`; not enforced) |
| `MICROTUN_SIGNER_URL` | `local:` (default) or a signer service |
| `MICROTUN_BOOT_KEY_ID`, `ESP_SBV2_KEY_ID` | Application (P-256) and boot-chain (RSA-3072) key IDs |
| `MICROTUN_BOOT_PUBKEYS` | Application public keys trusted by microtun boot (default: the local application signing key) |

**Boot-chain build:**

| Variable | Purpose |
|---|---|
| `ESP_BOOTLOADER_PROFILE` | `dev` (default; JTAG and spare key slots stay open) or `release` (disables JTAG, revokes spare slots) |
| `ESP_IDF_BOOTLOADER_BIN` | Prebuilt unsigned second stage, built for 4 MiB of flash; skips Docker |

`SLOT=image-1` only places an image in the secondary slot; it does not request
an upgrade. The trailer that triggers one is written by the running
application through the
[updater](../../microtun-boot-core/README.md#updating-from-an-application), not
by the flash tool.

For production, use a signer service with your own key IDs and set
`MICROTUN_BOOT_PUBKEYS` to your pinned application key.

### Secure Download Mode

Neither bootloader profile enables Secure Download Mode, but a chip may have
it from a manually burned `ENABLE_SECURITY_DOWNLOAD` eFuse (for example, at
manufacture). On such a chip, **flash through a USB-to-UART adapter**, not the
USB-C port.

The reason is the RTC (LP) watchdog. microtun boot arms it for 15 s, and a
USB-Serial-JTAG reset into the ROM downloader leaves it running. Normally the
flash tool turns it off with `WRITE_REG` after connecting. Secure Download
Mode rejects `WRITE_REG`, so the tool can only warn, and any write longer than
the remaining watchdog time is cut off by a chip reset. A reset through `EN`
also resets the watchdog.

The Mini has no on-board USB-to-UART bridge, so use an external 3.3 V
adapter on UART0: adapter TX to GPIO12 (U0RXD, the header's `RX`), adapter
RX to GPIO11 (U0TXD, `TX`), and GND. Unless the adapter's DTR/RTS drive `EN`
and GPIO28, enter download mode by hand: hold **BOOT** (GPIO28), press and
release **RESET**, then release **BOOT**.

```sh
export ESPPORT=/dev/ttyUSB0   # the adapter; it is never auto-detected
```

The UART runs at 115200 baud, so large writes are much slower than over
USB-Serial-JTAG. microtun boot still logs on the USB-C port, so keep it
connected if you want to watch the boot.

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

> Use a **dedicated, expendable board.** The tests sign with the development
> RSA key and, on a blank chip, enroll it permanently with no confirmation.

Connect the board's USB-C port, then:

```sh
export MICROTUN_BOOT_ESP_PORT=/dev/serial/by-id/usb-Espressif_USB_JTAG_serial_debug_unit_YOUR_DEVICE_ID
cargo test -p microtun-boot-hil --features esp32c5-mini,smoke -- --test-threads=1
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
