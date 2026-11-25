# ESP32-C5 Mini blinky

A minimal Embassy application for the [microtun boot](../../README.md) bootloader
on an ESP32-C5 Mini, and a template for your own.

- Blinks the user LED (GPIO23): fast while the image is an unconfirmed
  trial, a slow heartbeat once it is permanent.
- The **BOOT button** (GPIO28) logs what microtun boot handed over: mode, slot, maximum
  image size and bootloader version.
- **Confirms** itself after a 5 s self-test. If it resets or panics before
  then, microtun boot reverts the upgrade.

By default the image is signed remotely with the microtun boot **development** key
`microtun-boot-dev-p256-v1`, so it boots on the released `dev` boot chain. You
can instead [use your own local keys](#use-your-own-keys).

## Prerequisites

Run these from this directory in a checkout of the microtun boot repository:

```sh
rustup target add riscv32imac-unknown-none-elf
python3 -m pip install imgtool
(cd ../.. && cargo install --path microtun-boot-esp/tools --locked)  # host tool, from the repo root
cargo install --locked --git https://github.com/microtun/microtun-signer microtun-signer
```

Prebuilt Linux `microtun-signer` binaries are on its GitHub releases. To sign
with the development key, your GitHub account must be allowed by the signer's
policy; the first signature opens GitHub's device login. How signing works
end to end, local or remote, is in [SIGNING.md](../../SIGNING.md).

## 1. Flash the dev boot chain (once per board)

> **Irreversible.** On first boot Secure Boot V2 is enabled and the **dev**
> RSA key is burned into eFuse. From then on the board only boots boot chains
> signed with that key. Use a board you are willing to dedicate.

Download `esp32c5-mini-dev-boot-chain.sbv2.bin` from a
[microtun boot release](https://github.com/microtun/microtun-boot/releases),
connect the board through its USB-C port and flash the boot chain followed by
the application:

```sh
export ESPPORT=/dev/ttyACM0
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-esp --board esp32c5-mini \
  flash-bootloader --signed-bin \
  esp32c5-mini-dev-boot-chain.sbv2.bin
cargo run --release      # sign, flash image-0 and open the monitor (Ctrl+C to exit)
```

microtun boot reboots until the application is installed; flashing the
application last lets you watch the first boot.

## 2. Build, sign, flash

```sh
cargo run --release                          # IMAGE_VERSION defaults to 0.1.0
IMAGE_VERSION=0.2.0 cargo run --release
NO_FLASH=1 IMAGE_VERSION=0.2.0 cargo run --release   # sign only, e.g. for an OTA payload
# -> ../../target/riscv32imac-unknown-none-elf/release/esp32c5-mini-blinky.signed.bin
```

`cargo run` writes the primary slot directly, so the image boots as permanent:

```
INFO - hello from image 0.1.0+0 in Primary (mode SwapScratch)
INFO - image already permanent
INFO - alive
```

A real upgrade is staged by the running application into `image-1` with the
[updater API](../../microtun-boot-core/README.md#updating-from-an-application),
followed by `mark_for_test()` and a reset. The new image then boots as a trial
and flashes amber until it confirms itself. This example has no update
transport.

> **Anti-rollback (temporarily disabled):** `SECURITY_COUNTER` (default `0`) is
> signed metadata only on C5 with esp-hal 1.2.2. It does not block older
> images. Leave it at `0` until hardware rollback support returns; see the
> [board README](../../boards/esp32c5-mini/README.md#security-counter).

## Use your own keys

Instead of the released boot chain, you can build one that trusts your local
keys. No remote signer is needed, but a board already provisioned with the
`dev` boot chain **cannot switch** to your keys: use a fresh board. Follow the
[board README](../../boards/esp32c5-mini/README.md) for setup and back
up your key store first.

From the repository root, with these exports kept for every step:

```sh
export MICROTUN_SIGNER_URL=local:
export MICROTUN_SIGNER_GITHUB_LOGIN=false
export MICROTUN_SIGNER_KEY_ID=microtun-boot-local-p256
export MICROTUN_BOOT_KEY_ID=microtun-boot-local-p256
export ESP_SBV2_KEY_ID=microtun-boot-local-esp32c5-sbv2-rsa3072
unset MICROTUN_BOOT_PUBKEYS MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY  # no pinned dev key, no key override

(cd boards/esp32c5-mini/microtun-boot && NO_FLASH=1 cargo run --release)

cd examples/esp32c5-mini-blinky
MICROTUN_BOOT_OTP_BURN=YES microtun-boot-tools-esp --board esp32c5-mini \
  flash-bootloader --signed-bin \
  ../../target/riscv32imac-unknown-none-elf/release/boot-chain.sbv2.bin
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
- **Release boot chain:** boards provisioned with the `release` boot chain use
  `microtun-boot-release-p256-v1` instead. A dev board cannot switch. That
  profile disables JTAG, so the HIL tests, which read the security counter
  over JTAG, cannot run on such a board.

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

Copy this example and keep its `build.rs`, the `esp_app_desc!()` call (the
ESP-IDF application descriptor microtun boot's image conversion expects), the
watchdog task and the confirm-after-self-test logic. The full checklist is in
the [board README](../../boards/esp32c5-mini/README.md#writing-an-application).
