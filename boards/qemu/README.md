# QEMU (test target)

**Not a real board.** A Cortex-M3 platform on a patched QEMU `mps2-an385`
machine. It exists so
[`microtun-boot-hil`](../../testing/microtun-boot-hil/README.md) can run the full
regression suite quickly, in parallel and without hardware. Nothing here
ships.

## Setup

From the repository root:

```sh
rustup target add thumbv7m-none-eabi
python3 -m pip install imgtool
cargo install --path microtun-boot-qemu/tools --locked
microtun-signer local init --key-id microtun-boot-local-p256 --algorithm p256
```

Build the patched QEMU once as described in [`qemu/README.md`](qemu/README.md),
then point the tools at it:

```sh
export MICROTUN_BOOT_QEMU_BIN="$PWD/target/qemu/qemu-system-arm"
```

## Run the tests

```sh
cargo test -p microtun-boot-hil --features qemu,regression
```

Each test uses its own isolated virtual board.

## Run manually

The workflow matches the hardware boards, except that you start the emulator
yourself:

```sh
cd boards/qemu/microtun-boot
cargo run --release                       # flash the bootloader
cd ../firmware
IMAGE_VERSION=1.0.0 cargo run --release   # sign and flash the application
cd ../../..
microtun-boot-tools-qemu run                  # serial on stdio, Ctrl+C to stop
```

"Flashing" writes to a virtual flash image, `target/qemu/flash.bin`. The
virtual security counter lives in `target/qemu/efuses.bin` and survives
reflashes; delete it to reset the counter. Do not flash while the emulator is
running. No `MICROTUN_BOOT_OTP_BURN` acknowledgement is needed, since nothing is
burned.

The same variables as the hardware tools apply (`IMAGE_VERSION`,
`SECURITY_COUNTER`, `NO_FLASH`, `MICROTUN_SIGNER*`), plus
`MICROTUN_BOOT_QEMU_FLASH` to use another virtual board.

## Limits

- Killing QEMU right after a flash command is not a true power cut, because
  QEMU writes its backing files asynchronously.
- The emulated eFuses and TRNG are not a security boundary or real entropy.
- Chip-specific code (ESP32-C3 loader and MMU, RP2350 bootrom handoff, real
  flash timing) is not covered; the physical smoke suites cover it.
