# RP2350 Pico 2 blinky

A minimal Embassy application for the [microtun boot](../../README.md) bootloader
on a Raspberry Pi Pico 2 or Pico 2 W, and a template for your own.

- Blinks the on-board LED: **fast** while the image is an unconfirmed trial,
  **slow** once it is permanent.
- Logs what microtun boot handed over at startup: mode, slot, maximum image size
  and bootloader version.
- **Confirms** itself after a 5 s self-test. If it resets before then,
  microtun boot reverts the upgrade.

By default the image is signed remotely with the microtun boot **development** key
`microtun-boot-dev-p256-v1`, so it boots on the released `dev` bootloader. You
can instead [use your own local keys](#use-your-own-keys).

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

Run these from this directory in a checkout of the microtun boot repository:

```sh
rustup target add thumbv8m.main-none-eabihf
python3 -m pip install imgtool
(cd ../.. && cargo install --path microtun-boot-rp/tools --locked)  # host tool, from the repo root
cargo install --locked --git https://github.com/microtun/microtun-signer microtun-signer
```

Prebuilt Linux `microtun-signer` binaries are on its GitHub releases. To sign
with the development key, your GitHub account must be allowed by the signer's
policy; the first signature opens GitHub's device login. How signing works
end to end, local or remote, is in [SIGNING.md](../../SIGNING.md).

## 1. Flash the dev bootloader (once per board)

> **Irreversible.** On its first successful boot microtun boot enrolls RP2350
> secure boot in OTP with the **dev** ROM key. From then on the board only
> boots bootloaders signed with that key. Use a board you are willing to
> dedicate, and see [Secure boot](../../boards/rp2350-pico2/README.md#secure-boot).

Download `rp2350-pico2-dev-microtun-boot.signed.bin` from a
[microtun boot release](https://github.com/microtun/microtun-boot/releases), then
flash the bootloader followed by the application:

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

> **Anti-rollback:** `SECURITY_COUNTER` (default `0`) is independent of
> `IMAGE_VERSION`. Booting an image with a higher counter permanently blocks
> older images. Leave it at `0` unless you are testing that policy.

## Use your own keys

Instead of the released bootloader, you can build one that trusts your local
keys. No remote signer is needed, but a board already enrolled with the `dev`
ROM key **cannot switch** to yours: use a fresh board. Follow the
[board README](../../boards/rp2350-pico2/README.md) for setup and back up your
key store first.

From the repository root, with these exports kept for every step:

```sh
export MICROTUN_SIGNER_URL=local:
export MICROTUN_SIGNER_GITHUB_LOGIN=false
export MICROTUN_SIGNER_KEY_ID=microtun-boot-local-p256
export MICROTUN_BOOT_KEY_ID=microtun-boot-local-p256
export RP2350_SECURE_BOOT_KEY_ID=microtun-boot-local-rp2350-rom-secp256k1
unset MICROTUN_BOOT_PUBKEYS MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY  # no pinned dev key, no key override

(cd boards/rp2350-pico2/microtun-boot && NO_FLASH=1 cargo run --release)

cd examples/rp2350-pico2-blinky
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-rp flash-bootloader --signed-bin \
  ../../target/thumbv8m.main-none-eabihf/release/rp2350-pico2-microtun-boot.signed.bin
cargo run --release
```

The exports override the remote development key selected in this example's
`.cargo/config.toml`.

## Signing settings

Defaults are in `.cargo/config.toml`; shell variables override them. The
notes below matter once this example becomes firmware of your own, built in
CI or released:

- **CI:** set `MICROTUN_SIGNER_GITHUB_LOGIN=false` (it must be `true` or
  `false`, not `1`) and authenticate with `MICROTUN_SIGNER_TOKEN` or GitHub
  Actions OIDC (`permissions: id-token: write`).
- **Release bootloader:** boards provisioned with the `release` bootloader use
  `microtun-boot-release-p256-v1` instead, and a dev board cannot switch. That
  bootloader **permanently disables SWD** on first boot, so flash the
  application before the bootloader and plan updates through your own
  transport.

### Pin the signer's key

By default each signature is only checked against whatever public key the
service returns. To require the published key, fetch it once, check its
fingerprint and point `MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY` at it:

```sh
microtun-signer public-key --url https://signer.microtun.dev \
  --key-id microtun-boot-dev-p256-v1 > microtun-boot-dev-p256-v1.pub.pem
openssl pkey -pubin -in microtun-boot-dev-p256-v1.pub.pem -outform DER | sha256sum
export MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY="$PWD/microtun-boot-dev-p256-v1.pub.pem"
```

Compare the digest with the published fingerprints in
[SIGNING.md](../../SIGNING.md#published-microtun-boot-keys).

## Writing your own application

Copy this example and keep its `build.rs`, the `imagedef-none` and `--nmagic`
settings (the application is started by microtun boot, not the ROM, and must link
tightly into the window microtun boot maps), the watchdog feeding and the
confirm-after-self-test logic. The full checklist is in the
[board README](../../boards/rp2350-pico2/README.md#writing-an-application).
