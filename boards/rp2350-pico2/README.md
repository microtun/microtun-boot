# RP2350 Pico 2 / Pico 2 W

On RP2350, microtun boot is the only stage after the mask ROM:

1. The ROM loads microtun boot from flash offset `0x0` into SRAM and, once secure
   boot is enrolled, checks its **secp256k1** signature against the key
   fingerprint in OTP `BOOTKEY0`.
2. microtun boot verifies the application (MCUboot format, ECDSA **P-256**) in
   `image-0` or `image-1`, performs any pending upgrade and starts the
   application.

The chosen slot is always mapped to the same address, so one firmware build
runs from either slot.

> **Every build enrolls ROM secure boot in OTP on its first successful boot.**
> This is irreversible: the board is then bound to that ROM key forever. Read
> [Secure boot](#secure-boot) before flashing.

## Requirements and limitations

- Pico 2 or Pico 2 W (RP2350A) with **4 MiB** flash. Arm cores only:
  microtun boot does not support the RISC-V cores.
- An SWD probe such as the Raspberry Pi Debug Probe. All flashing uses SWD,
  not BOOTSEL/UF2.
- For logs, a 3.3 V serial adapter on **GPIO0** (UART0 TX, Pico pin 1),
  115200 8N1. The Debug Probe's UART port works.
- BOOTSEL/PICOBOOT, the ROM rollback counter and permanent OTP page locks are
  not configured. See the known limitations in
  [THREAT-MODEL.md](../../THREAT-MODEL.md#known-limitations) (L4, L9, L10).

## Flash layout

Defined in `layout/partitions.json`, offsets relative to `0x10000000`:

| Region | Offset | Size |
|---|---|---|
| `bootloader` | `0x0` | 128 KiB |
| `image-0` (primary) | `0x20000` | 1920 KiB |
| `image-1` (secondary) | `0x200000` | 1920 KiB |
| `image-scratch` | `0x3E8000` | 64 KiB |
| `storage` (yours) | `0x3F8000` | 32 KiB |

The largest signed application is **1912 KiB**: the 1920 KiB slot minus the
MCUboot swap-status trailer, rounded down to whole 4 KiB sectors. No RP2350
partition table is written; ROM A/B partitions are not used.

## Setup

```sh
rustup target add thumbv8m.main-none-eabihf
python3 -m pip install imgtool
cargo install --path microtun-boot-rp/tools --locked
microtun-signer local init --key-id microtun-boot-local-p256 --algorithm p256
microtun-signer local init --key-id microtun-boot-local-rp2350-rom-secp256k1 --algorithm secp256k1
```

The secp256k1 ROM key is bound to every board you enroll with it, and every
future bootloader for those boards must be signed with it. **Back up the
local key store** (`microtun-signer local path`) before provisioning, and
never reuse the key for anything else. To use a ROM key of your own:

```sh
microtun-signer local import --key-id my-rp2350-rom --algorithm secp256k1 --pem rp2350-rom.pem
export RP2350_SECURE_BOOT_KEY_ID=my-rp2350-rom
```

## Build and sign without flashing

Set `NO_FLASH=1` on either command. No board or OTP acknowledgement is
needed. The outputs land in `target/thumbv8m.main-none-eabihf/release/` at
the repository root:

| File | Flash address |
|---|---|
| `rp2350-pico2-microtun-boot.signed.bin` | `0x10000000` |
| `rp2350-pico2-firmware.signed.bin` | `image-0`, `0x10020000` |

Two cargo features on `rp2350-pico2-microtun-boot` are chosen at build time: the
upgrade strategy (swap-using-scratch by default, or `overwrite-only`,
`direct-xip`, `direct-xip-revert`) and `production`, the hardened OTP policy
described in [Secure boot](#secure-boot). Add `production` on top of the
default features; do not disable them.

The ELFs are not flashable as-is: the bootloader needs ROM sealing and the
application an MCUboot signature. `microtun-boot-tools-rp --help` lists the
lower-level commands (`check`, `sign`, `sign-bootloader` and others).

## Flash a signed release binary

To flash an already signed bootloader, such as a release artifact, without
any signing key:

```sh
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-rp flash-bootloader --signed-bin \
  rp2350-pico2-release-microtun-boot.signed.bin
```

The tool checks the signature and OTP profile first. With `NO_FLASH=1` it
only verifies. It cannot check that the file's key matches the key already
enrolled on the board.

## Writing an application

Start from the [blinky example](../../examples/rp2350-pico2-blinky/README.md),
not `boards/rp2350-pico2/firmware`: the board firmware contains
unauthenticated [test hooks](../../testing/microtun-boot-test-hooks/README.md) and
must never ship. An application must:

- Depend on [`microtun-boot-rp`](../../microtun-boot-rp/README.md) and the board layout
  crate, enable `embassy-rp`'s `imagedef-none`, and call
  `microtun_boot_build::configure_rp2350_application(MAX_IMAGE_SIZE)` in
  `build.rs` instead of providing its own `memory.x`.
- Enable its own interrupts. microtun boot hands off with all NVIC interrupts
  disabled and OTP pages 1, 2 and 32 write-locked until reset.
- Keep feeding the watchdog, which is still running at handoff.
- Leave QMI ATRANS1 (the flash address translation that maps the running slot
  to the application window) alone.
- Start core 1 only through Embassy's multicore API, so flash operations can
  pause it.
- Confirm a trial upgrade only after its own health checks. A reset before
  confirmation reverts the upgrade.

## Configuration

**Flashing:**

| Variable | Purpose |
|---|---|
| `RPPORT` | Serial console for the monitor (default: the SWD probe's serial port) |
| `MICROTUN_BOOT_OTP_BURN=YES` | Required to flash the bootloader |
| `NO_FLASH=1` | Build and sign only |
| `PROBE_RS_PROBE`, `PROBE_RS_SPEED_KHZ` | Probe selection and SWD speed |

**Signing** (see [SIGNING.md](../../SIGNING.md)):

| Variable | Purpose |
|---|---|
| `IMAGE_VERSION` | Application image version (default `0.0.0`) |
| `SECURITY_COUNTER` | Anti-rollback epoch of the image (default `0`; see below) |
| `MICROTUN_SIGNER_URL` | `local:` (default) or a signer service |
| `MICROTUN_BOOT_KEY_ID`, `RP2350_SECURE_BOOT_KEY_ID` | Application (P-256) and ROM (secp256k1) key IDs |
| `MICROTUN_BOOT_PUBKEYS` | Application public keys trusted by microtun boot (default: the local application signing key) |

## Secure boot

Two independent keys protect the chain: the ROM checks microtun boot against the
secp256k1 key in OTP, and microtun boot checks the application against the P-256
keys compiled into it.

On every boot microtun boot enrolls or verifies the OTP state, enables secure boot
last, and then reboots. An enrolled board stays bound to its ROM key.

| | Default | `production` |
|---|---|---|
| ROM authenticates microtun boot | Yes | Yes |
| Spare key slots `BOOTKEY1..3` | Left open | Revoked |
| Arm SWD debugging | Enabled | **Permanently disabled** |
| Glitch detectors | Armed | Armed |

**Promoting to production:** flash a `production` build, signed with the same
ROM key, to a development board while SWD still works. There is no way back,
and a board with extra keys enrolled in `BOOTKEY1..3` must revoke them first.

- **First boot is trust-on-first-use.** Whoever controls the first image
  chooses the key, so do initial flashing in a controlled environment.
- An interrupted OTP burn can leave a board unbootable. Validate the full
  chain on a sacrificial board first.
- Boards whose supply or clock trips the glitch detectors cannot boot once
  they are armed. See
  [fault-injection hardening](../../THREAT-MODEL.md#fault-injection-hardening).
- If OTP holds unexpected keys or flags, microtun boot refuses to boot and leaves
  OTP unchanged.

## Security counter

The application rollback floor uses all of **OTP page 32**, giving 504
epochs. Raising `SECURITY_COUNTER` and booting that image permanently blocks
older images, so reserve it for security fixes.

## Hardware-in-the-loop tests

> Use a **dedicated, expendable board.** The tests sign with the local
> development ROM key (or `RP2350_SECURE_BOOT_KEY_ID`) and, on a blank chip,
> enroll it permanently with no confirmation.

```sh
export MICROTUN_BOOT_RP235X_PORT=/dev/serial/by-id/usb-Arm_DAPLink_CMSIS-DAP_YOUR_DEVICE_ID
cargo test -p microtun-boot-hil --features rp2350-pico2,smoke -- --test-threads=1
```

`MICROTUN_BOOT_RP235X_PORT` is the UART console. If you change the
[flash algorithm](flash-algorithm/README.md) sources, rebuild it first. See
[`microtun-boot-hil`](../../testing/microtun-boot-hil/README.md).
