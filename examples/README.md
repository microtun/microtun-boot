# Examples

Standalone Embassy applications that run under the
[microtun boot](../README.md) bootloader, and templates for your own
firmware. All four examples do the same thing; only the hardware details
differ:

- Blinks the LED **fast** while the image is an unconfirmed trial, **slow**
  once it is permanent.
- Logs what microtun boot handed over — mode, slot, maximum image size and
  bootloader version — continuously on the **BOOT button** where the board
  has one, or at startup.
- **Confirms** itself after a 5 s self-test. If it resets or panics before
  then, microtun boot reverts the upgrade.

| Example | Board | LED | Rust target |
|---|---|---|---|
| [esp32c3-supermini-blinky](esp32c3-supermini-blinky/README.md) | [ESP32-C3 SuperMini](../boards/esp32c3-supermini/README.md) | blue LED, GPIO8 | `riscv32imc-unknown-none-elf` |
| [esp32c5-mini-blinky](esp32c5-mini-blinky/README.md) | [ESP32-C5 Mini](../boards/esp32c5-mini/README.md) | user LED, GPIO23 | `riscv32imac-unknown-none-elf` |
| [esp32c6-supermini-blinky](esp32c6-supermini-blinky/README.md) | [ESP32-C6 SuperMini](../boards/esp32c6-supermini/README.md) | RGB LED, GPIO8 (RMT): amber trial, green permanent | `riscv32imac-unknown-none-elf` |
| [rp2350-pico2-blinky](rp2350-pico2-blinky/README.md) | [RP2350 Pico 2 / Pico 2 W](../boards/rp2350-pico2/README.md) | on-board LED | `thumbv8m.main-none-eabihf` |

By default every example's image is signed remotely with the microtun boot
**development** key `microtun-boot-dev-p256-v1`, so it boots on the released
`dev` boot chain. You can instead [use your own local keys](#use-your-own-keys).

## Prerequisites

Run these from the example's directory, in a checkout of the microtun boot
repository; the target and host tool come from the table above and the board
README:

```sh
rustup target add <target>
python3 -m pip install imgtool
(cd ../.. && cargo install --path microtun-boot-esp/tools --locked)   # or microtun-boot-rp/tools
cargo install --locked --git https://github.com/microtun/microtun-signer microtun-signer
```

Prebuilt Linux `microtun-signer` binaries are on its GitHub releases. To sign
with the development key, your GitHub account must be allowed by the signer's
policy; the first signature opens GitHub's device login. How signing works
end to end, local or remote, is in [SIGNING.md](../SIGNING.md).

## 1. Flash the dev boot chain (once per board)

> **Irreversible.** On first boot the board's secure boot is enabled and the
> **dev** key is burned into eFuse (ESP32) or OTP (RP2350). From then on the
> board only boots boot chains signed with that key. Use a board you are
> willing to dedicate.

Download the `dev` boot chain for your board from a
[microtun boot release](https://github.com/microtun/microtun-boot/releases),
then flash the boot chain followed by the application — the exact commands
are in the example's README. Flashing the application last lets you watch the
first boot: microtun boot reboots until a valid application is installed
(and on the RP2350, only enrolls OTP once one is).

## 2. Build, sign, flash

```sh
cargo run --release                          # IMAGE_VERSION defaults to 0.1.0
IMAGE_VERSION=0.2.0 cargo run --release
NO_FLASH=1 IMAGE_VERSION=0.2.0 cargo run --release   # sign only, e.g. for an OTA payload
# -> ../../target/<target>/release/<example>.signed.bin
```

`cargo run` writes the primary slot directly, so the image boots as
permanent:

```
INFO - hello from image 0.1.0+0 in Primary (mode SwapScratch)
INFO - image already permanent
INFO - alive
```

A real upgrade is staged by the running application into `image-1` with the
[updater API](../microtun-boot-core/README.md#updating-from-an-application),
followed by `mark_for_test()` and a reset. The new image then boots as a
trial, blinks fast and confirms itself after its 5 s self-test; a reset
during the self-test reverts to the previous image. The examples have no
update transport.

> **Anti-rollback:** `SECURITY_COUNTER` (default `0`) is independent of
> `IMAGE_VERSION`. On boards with a hardware counter, booting an image with a
> higher counter permanently blocks older images. Leave it at `0` unless you
> are testing that policy; see each board README's security counter section
> ([ESP32](../boards/README-ESP32.md#security-counter),
> [RP2350](../boards/rp2350-pico2/README.md#security-counter)).

## Use your own keys

Instead of the released boot chain, you can build one that trusts your local
keys. No remote signer is needed, but a board already provisioned with the
`dev` boot chain **cannot switch** to your keys: use a fresh board. Follow
the board README for setup and back up your key store first.

From the repository root, with these exports kept for every step:

```sh
export MICROTUN_SIGNER_URL=local:
export MICROTUN_SIGNER_GITHUB_LOGIN=false
export MICROTUN_SIGNER_KEY_ID=microtun-boot-local-p256
export MICROTUN_BOOT_KEY_ID=microtun-boot-local-p256
export <ROM-KEY-VAR>=<ROM-KEY-ID>   # from the table below
unset MICROTUN_BOOT_PUBKEYS MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY  # no pinned dev key, no key override
```

| Example | ROM key variable | Local key ID |
|---|---|---|
| `esp32c3-supermini-blinky` | `ESP_SBV2_KEY_ID` | `microtun-boot-local-esp32c3-sbv2-rsa3072` |
| `esp32c5-mini-blinky` | `ESP_SBV2_KEY_ID` | `microtun-boot-local-esp32c5-sbv2-rsa3072` |
| `esp32c6-supermini-blinky` | `ESP_SBV2_KEY_ID` | `microtun-boot-local-esp32c6-sbv2-rsa3072` |
| `rp2350-pico2-blinky` | `RP2350_SECURE_BOOT_KEY_ID` | `microtun-boot-local-rp2350-rom-secp256k1` |

Then build the boot chain, flash it and run the application — the exact
commands are in each example's README. The exports override the remote
development key selected in each example's `.cargo/config.toml`.

## Signing settings

Defaults are in each example's `.cargo/config.toml`; shell variables
override them. For building firmware of your own in CI or releasing it —
signer authentication, the `release` boot chain's key, and pinning the
signer's public key — see
[SIGNING.md](../SIGNING.md#signing-example-firmware-in-ci-and-releases).

## Writing your own application

Copy an example and keep its `build.rs`, the watchdog task and the
confirm-after-self-test logic. On the ESP32 also keep the `esp_app_desc!()`
call (the ESP-IDF application descriptor microtun boot's image conversion
expects); on the RP2350 keep the `imagedef-none` and `--nmagic` settings (the
application is started by microtun boot, not the ROM, and must link tightly
into the window microtun boot maps). The full checklists are in the board
documentation: [ESP32](../boards/README-ESP32.md#writing-an-application),
[RP2350](../boards/rp2350-pico2/README.md#writing-an-application).
