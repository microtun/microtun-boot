# Signing with microtun-signer

microtun boot's host-side image tools sign everything through the [microtun-signer](https://github.com/microtun/microtun-signer) command-line client. By default it runs in **local mode** (`MICROTUN_SIGNER_URL` unset, i.e. `local:`): development keys live in microtun-signer's local store on your machine, with no service, no sign-in and no key files in this repository. For releases, point the same tools at a signer service, which keeps the private keys remote. Nothing in the embedded firmware contacts either.

The tools never read a private key themselves. They ask microtun-signer for the public key and a signature over a SHA-256 digest, and verify the result before writing any image.

## Development keys (default)

Install microtun-signer 0.13 or later (local mode) on `PATH`. Create the keys you need directly with its CLI, once per machine:

```sh
# All boards: MCUboot application signing (QEMU needs only this key).
microtun-signer local init --key-id microtun-boot-local-p256 --algorithm p256

# ESP32-C3 only: Secure Boot V2 RSA-3072 boot chain.
microtun-signer local init --key-id microtun-boot-local-esp32c3-sbv2-rsa3072 --algorithm rsa-pss

# ESP32-C5 only: Secure Boot V2 RSA-3072 boot chain.
microtun-signer local init --key-id microtun-boot-local-esp32c5-sbv2-rsa3072 --algorithm rsa-pss

# ESP32-C6 only: Secure Boot V2 RSA-3072 boot chain.
microtun-signer local init --key-id microtun-boot-local-esp32c6-sbv2-rsa3072 --algorithm rsa-pss

# RP2350 only: ROM verification of the bootloader.
microtun-signer local init --key-id microtun-boot-local-rp2350-rom-secp256k1 --algorithm secp256k1
```

`microtun-signer local init` creates a key only if it does not exist yet and never replaces one, so it is safe to run again. The microtun boot board tools do not manage signing keys. The key IDs are the defaults the tools use with a `local:` URL; each purpose has its own variable to choose another key:

| Default key ID | Algorithm | Signs | Override |
|---|---|---|---|
| `microtun-boot-local-p256` | P-256 | MCUboot application images, all boards; trusted by default bootloader builds | `MICROTUN_BOOT_KEY_ID` |
| `microtun-boot-local-esp32c3-sbv2-rsa3072` | RSA-3072 (PSS) | ESP32-C3 Secure Boot V2 stages | `ESP_SBV2_KEY_ID` |
| `microtun-boot-local-esp32c5-sbv2-rsa3072` | RSA-3072 (PSS) | ESP32-C5 Secure Boot V2 stages | `ESP_SBV2_KEY_ID` |
| `microtun-boot-local-esp32c6-sbv2-rsa3072` | RSA-3072 (PSS) | ESP32-C6 Secure Boot V2 stages | `ESP_SBV2_KEY_ID` |
| `microtun-boot-local-rp2350-rom-secp256k1` | secp256k1 | RP2350 ROM bootloader | `RP2350_SECURE_BOOT_KEY_ID` |

A command's key ID is the first of `--signer-key-id`, its purpose's variable, `MICROTUN_SIGNER_KEY_ID` and, with a `local:` URL, the default above. The purpose variables let one shell name, say, both an application key and a ROM key.

microtun-signer picks the store: `MICROTUN_SIGNER_LOCAL_DIR`, else `microtun-signer/local` in your user data directory (`~/.local/share` on Linux). It is outside any checkout, so the keys survive `git clean` and are shared by all your microtun boot checkouts. Use `MICROTUN_SIGNER_URL=local:/absolute/dir` for a separate store, and `microtun-signer local path` / `microtun-signer local list` to see what you have.

Local keys are unencrypted: **never ship anything signed with them.** A board whose eFuses or OTP were provisioned with a local ROM key depends on that key from then on; back up the store like any other key you cannot regenerate.

### Bootloader trust anchor

When `MICROTUN_BOOT_PUBKEYS` is unset, bootloader builds embed the public key of the application key, `MICROTUN_BOOT_KEY_ID`, `MICROTUN_SIGNER_KEY_ID` or `microtun-boot-local-p256`, from the local store (via `microtun-signer local path`). If `MICROTUN_SIGNER_URL` names a signer service instead, the build stops: set `MICROTUN_BOOT_PUBKEYS` to that key's pinned public key (see below). A build never trusts a key just because a service returned it.

### Migrating from `keys/`

Earlier versions kept development keys as PEM files under `keys/` and `boards/*/keys/`, selected with the paths `MICROTUN_BOOT_KEY`, `ESP_SBV2_KEY`, `RP2350_SECURE_BOOT_KEY` or `--key`. Those settings are gone; their key-ID counterparts are `MICROTUN_BOOT_KEY_ID`, `ESP_SBV2_KEY_ID` and `RP2350_SECURE_BOOT_KEY_ID`. To keep signing with existing keys (for example for a board already provisioned with them), import them into the local store **before** generating new ones with `microtun-signer local init`:

```sh
microtun-signer local import --key-id microtun-boot-local-p256 --algorithm p256 \
    --pem keys/dev-p256.pem
microtun-signer local import --key-id microtun-boot-local-esp32c3-sbv2-rsa3072 --algorithm rsa-pss \
    --pem boards/esp32c3-supermini/keys/dev-esp32c3-sbv2-rsa3072.pem
microtun-signer local import --key-id microtun-boot-local-rp2350-rom-secp256k1 --algorithm secp256k1 \
    --pem boards/rp2350-pico2/keys/dev-rp2350-rom-secp256k1.pem
```

The PEM files are copied, not moved; delete them once the import is done. A key kept elsewhere imports the same way, under any ID; point the matching `*_KEY_ID` variable at it.

## Select a signer service

Install the `microtun-signer` command-line client and configure access to the service (for example, a `MICROTUN_SIGNER_TOKEN` bearer token for GitHub Actions). Export:

```sh
export MICROTUN_SIGNER_URL=https://signer.example.com/
export MICROTUN_SIGNER_KEY_ID=my-p256-firmware-key
# One possible authentication mechanism:
export MICROTUN_SIGNER_TOKEN="$TOKEN"
# Or, for an interactive user, set MICROTUN_SIGNER_GITHUB_LOGIN=1.
```

`MICROTUN_SIGNER` can override the CLI executable name/path. With a service URL, a key ID (`MICROTUN_SIGNER_KEY_ID` or the purpose's variable) is required: the tools never substitute a development key ID. A signing failure is an error; there is no fallback to another key. Every signing command also accepts `--signer-url`, `--signer-key-id` and `--signer-github-login` in place of the variables.

**Recommended:** Pin the service's expected public signing key before releasing any binaries:

```sh
microtun-signer public-key --url "$MICROTUN_SIGNER_URL" \
    --key-id "$MICROTUN_SIGNER_KEY_ID" > firmware-p256.pub.pem
export MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY="$PWD/firmware-p256.pub.pem"
```

The tools compare the returned SubjectPublicKeyInfo with this file before signing (in local mode too). The file is public (no private signing PEM needed). Use separate key IDs, and corresponding expected-public-key files, for different algorithms.

Fetching the file from the same service you are trying to verify only helps if you check it once against a value you trust. Obtain each key's fingerprint out of band (from the signer operator, or from the key-creation record) and compare it with the downloaded file:

```sh
openssl pkey -pubin -in firmware-p256.pub.pem -outform DER | sha256sum
```

This SHA-256 of the DER SubjectPublicKeyInfo is the same fingerprint the release workflow pins (see [GitHub Actions release workflow](#github-actions-release-workflow)).

## Signing example firmware in CI and releases

The [blinky examples](examples/README.md) sign with the remote development
key by default (selected in each example's `.cargo/config.toml`; shell
variables override it). The notes below matter once an example becomes
firmware of your own, built in CI or released:

- **CI:** set `MICROTUN_SIGNER_GITHUB_LOGIN=false` (it must be `true` or
  `false`, not `1`) and authenticate with `MICROTUN_SIGNER_TOKEN` or GitHub
  Actions OIDC (`permissions: id-token: write`).
- **Release keys:** boards provisioned with the `release` boot chain (ESP32)
  or `release` bootloader (RP2350) verify `microtun-boot-release-p256-v1`
  signatures instead, and a dev-provisioned board cannot switch. The ESP32
  `release` profile disables JTAG, so the HIL tests, which read the security
  counter over JTAG, cannot run on such a board. The RP2350 `release`
  bootloader **permanently disables SWD** on first boot, so flash the
  application before the bootloader and plan updates through your own
  transport.

### Pinning the development key

By default each signature is only checked against whatever public key the
service returns. To require the published key, fetch it once, check its
fingerprint and point `MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY` at it:

```sh
microtun-signer public-key --url https://signer.microtun.dev \
  --key-id microtun-boot-dev-p256-v1 > microtun-boot-dev-p256-v1.pub.pem
openssl pkey -pubin -in microtun-boot-dev-p256-v1.pub.pem -outform DER | sha256sum
export MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY="$PWD/microtun-boot-dev-p256-v1.pub.pem"
```

Compare the digest with the development fingerprints pinned in the
[release workflow](#github-actions-release-workflow) before trusting it.

## Firmware: MCUboot images (ESP32-C3, ESP32-C5, ESP32-C6, RP2350, QEMU)

Use a **P-256 / secp256r1** key in microtun-signer (local default: `microtun-boot-local-p256`). All existing microtun boot bootloaders verify P-256; no new embedded signature format is introduced. The host tool exports imgtool's SHA-256 digest, has microtun-signer sign it, verifies the returned fixed-width `r || s` signature with the service's public key, converts it to DER, assembles the final image with `imgtool --fix-sig`, and independently verifies the signed image.

To trust this signing key on-device, embed its public key in the **microtun boot bootloader build** using the existing build-script setting:

```sh
export MICROTUN_BOOT_PUBKEYS="$PWD/firmware-p256.pub.pem"
# Rebuild the appropriate boards/<board>/microtun-boot binary with this setting.
```

`MICROTUN_BOOT_PUBKEYS` also accepts a comma-separated list of P-256 public keys for key rotation. Signing firmware with a key not already trusted by the device's bootloader will result in an unbootable image.

The board tools' `flash-app` commands use the environment variables directly. Set `IMAGE_VERSION`, `SECURITY_COUNTER`, `VID`, and `CID` as before.

## ESP32 ROM Secure Boot V2 boot chain

Use a **3072-bit RSA-PSS** key (SHA-256, 32-byte salt; local default: `microtun-boot-local-<chip>-sbv2-rsa3072`, for the chip of the `--board` the tool runs for). With a signer service, change the key ID and expected public key file to the RSA key before running the ROM signing commands. The host verifies the returned RSA-PSS signature, emits a standard ESP32 Secure Boot V2 signature sector, then verifies the result. `flash-bootloader` signs **both** the IDF second-stage bootloader and microtun boot with the same configured RSA key; their ROM public-key fields are checked for equality. `sign-sbv2` signs an individual image; `verify-sbv2` checks one against `--key` or, by default, the configured signer's public key.

```sh
export MICROTUN_SIGNER_KEY_ID=my-esp-rsa3072-key
export MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY="$PWD/esp-rsa3072.pub.pem"
# Then use microtun-boot-tools-esp --board <board> sign-sbv2 or flash-bootloader as usual.
```

Make sure the device's actual secure-boot eFuse digest corresponds to this **ROM RSA key**, not the separate MCUboot P-256 application signing key. Consult the ESP board's provisioning instructions before burning irreversible fuses.

## RP2350 ROM bootloader sealing

Use a **secp256k1** key for the RP2350 ROM stage (local default: `microtun-boot-local-rp2350-rom-secp256k1`). `sign-bootloader` and `flash-bootloader` fetch its public key and request a signature over the RP2350 ROM SHA-256 digest generated directly in Rust. The host converts the signer's fixed-width ECDSA signature to DER, verifies it, reseals the image and checks the RP2350 ROM metadata and optional OTP provisioning plan.

```sh
export MICROTUN_SIGNER_KEY_ID=my-rp2350-secp256k1-key
export MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY="$PWD/rp2350-secp256k1.pub.pem"
# Then use microtun-boot-tools-rp sign-bootloader or flash-bootloader.
```

## Keys per image

| Image | Algorithm | Local default key ID | Key ID variable |
|---|---|---|---|
| MCUboot firmware | P-256 | `microtun-boot-local-p256` | `MICROTUN_BOOT_KEY_ID` |
| ESP32-C3 Secure Boot V2 stages | RSA-3072 | `microtun-boot-local-esp32c3-sbv2-rsa3072` | `ESP_SBV2_KEY_ID` |
| ESP32-C5 Secure Boot V2 stages | RSA-3072 | `microtun-boot-local-esp32c5-sbv2-rsa3072` | `ESP_SBV2_KEY_ID` |
| ESP32-C6 Secure Boot V2 stages | RSA-3072 | `microtun-boot-local-esp32c6-sbv2-rsa3072` | `ESP_SBV2_KEY_ID` |
| RP2350 ROM bootloader | secp256k1 | `microtun-boot-local-rp2350-rom-secp256k1` | `RP2350_SECURE_BOOT_KEY_ID` |

`MICROTUN_SIGNER_KEY_ID` applies to whichever command you run and loses to the per-image variables. For a typical release, you will use **two separate remote key IDs**: one for MCUboot application firmware and one for the board-specific ROM bootloader. Configure and verify each key's public trust anchor *before* flashing or provisioning devices.

CI creates fresh development keys in a per-job local store (`.github/actions/dev-signing-keys`) and never uploads them.

## GitHub Actions release workflow

On a version tag, `.github/workflows/release.yml` builds the bootloaders without signing keys and signs them in the protected `firmware-signing` job via `microtun-signer`. For RP2350, that job runs `microtun-boot-tools-rp flash-bootloader` with `NO_FLASH=1`, producing `rp2350-pico2-microtun-boot.signed.bin` beside each staged ELF. It then copies the signed binaries to `dist/rp2350-pico2-{dev,release}-microtun-boot.signed.bin` for release publishing. This offline path performs ROM sealing and verification without contacting a debug probe, flashing a board, resetting hardware, or asking for OTP provisioning approval. The ESP32-C3, ESP32-C5 and ESP32-C6 boot chains are signed the same way, with `microtun-boot-tools-esp --board <board> flash-bootloader` and `NO_FLASH=1`.

To flash a previously signed CI release image without signing again, pass
`--signed-bin` to the board's `flash-bootloader` command. See "Flash a signed
release binary" in the
[ESP32 board documentation](boards/README-ESP32.md#flash-a-signed-release-binary)
and the [RP2350 README](boards/rp2350-pico2/README.md#flash-a-signed-release-binary).
Verification uses the public key embedded in the file; check the
release key identity separately against the actual board provisioning.

The release pipeline publishes **bootloader images**, not signed example application firmware. The workflow defines all eight microtun-signer key IDs directly in its top-level `env` block (no GitHub repository variables are required):

| Purpose | Development ID | Release ID |
| --- | --- | --- |
| P-256 firmware verification / MCUboot | `microtun-boot-dev-p256-v1` (`MICROTUN_BOOT_DEV_KEY_ID`) | `microtun-boot-release-p256-v1` (`MICROTUN_BOOT_RELEASE_KEY_ID`) |
| ESP32-C3 Secure Boot V2 | `microtun-boot-dev-esp32c3-sbv2-rsa3072-v1` | `microtun-boot-release-esp32c3-sbv2-rsa3072-v1` |
| ESP32-C5 Secure Boot V2 | `microtun-boot-dev-esp32c5-sbv2-rsa3072-v1` | `microtun-boot-release-esp32c5-sbv2-rsa3072-v1` |
| ESP32-C6 Secure Boot V2 | `microtun-boot-dev-esp32c6-sbv2-rsa3072-v1` | `microtun-boot-release-esp32c6-sbv2-rsa3072-v1` |
| RP2350 ROM | `microtun-boot-dev-rp2350-rom-k1-v1` | `microtun-boot-release-rp2350-rom-k1-v1` |

Each key ID has a pinned fingerprint in the same `env` block: the lowercase hex SHA-256 of the key's DER SubjectPublicKeyInfo. The variable name is the key ID variable with `_KEY_ID` replaced by `_KEY_SHA256`:

| Purpose | Development fingerprint | Release fingerprint |
| --- | --- | --- |
| P-256 firmware verification / MCUboot | `MICROTUN_BOOT_DEV_KEY_SHA256` | `MICROTUN_BOOT_RELEASE_KEY_SHA256` |
| ESP32-C3 Secure Boot V2 | `MICROTUN_BOOT_DEV_ESP32C3_SBV2_KEY_SHA256` | `MICROTUN_BOOT_RELEASE_ESP32C3_SBV2_KEY_SHA256` |
| ESP32-C5 Secure Boot V2 | `MICROTUN_BOOT_DEV_ESP32C5_SBV2_KEY_SHA256` | `MICROTUN_BOOT_RELEASE_ESP32C5_SBV2_KEY_SHA256` |
| ESP32-C6 Secure Boot V2 | `MICROTUN_BOOT_DEV_ESP32C6_SBV2_KEY_SHA256` | `MICROTUN_BOOT_RELEASE_ESP32C6_SBV2_KEY_SHA256` |
| RP2350 ROM | `MICROTUN_BOOT_DEV_RP2350_ROM_KEY_SHA256` | `MICROTUN_BOOT_RELEASE_RP2350_ROM_KEY_SHA256` |

The workflow never trusts a public key just because microtun-signer returned it:

- **Firmware keys.** The `signer-public-keys` job downloads the two P-256 public keys, checks each against its pinned fingerprint, and only then passes them to the bootloader builds through `MICROTUN_BOOT_PUBKEYS`. A mismatch stops the release before any bootloader is built, so the service cannot change which firmware key a device will trust.
- **ROM keys.** The protected `sign` job downloads the eight ESP32-C3, ESP32-C5, ESP32-C6 and RP2350 ROM public keys, checks each against its pinned fingerprint, and passes the file to `microtun-boot-tools-esp flash-bootloader` and `microtun-boot-tools-rp flash-bootloader` as `MICROTUN_SIGNER_EXPECTED_PUBLIC_KEY`. The tools fetch the key again before signing and refuse to continue if it differs from that file.

All-zero fingerprints are treated as unset placeholders and rejected, so a release cannot run until real values are filled in. To compute a fingerprint, run the following and confirm the result with the signer operator through a separate channel before committing it:

```sh
microtun-signer public-key --url https://signer.microtun.dev --key-id <key-id> \
  | openssl pkey -pubin -outform DER | sha256sum
```

Before tagging a release, provision all eight IDs in microtun-signer with the required key types, commit their eight fingerprints to the workflow, and configure the `firmware-signing` GitHub environment. Increment a key ID's `vN` suffix when rotating that key, not for each software release. Update its `_KEY_SHA256` pin in the same commit.
