# microtun boot Threat Model

## Purpose and basis

microtun boot guarantees that only application images signed by a trusted ECDSA P-256 key, and permitted by the configured policy, are executed by the boot chain it is part of. This document states which attacks that guarantee covers, which it does not, and what an integrator must provide for it to hold in production.

It is written for firmware integrators, security reviewers and product owners deciding whether microtun boot fits a threat model.

Terms used throughout: **slot** is a flash area holding one MCUboot-format image; **trailer** is the MCUboot metadata at the end of a slot; **security counter** is the hardware-backed monotonic rollback floor (`SEC_CNT`); **handoff** is the final jump from microtun boot into the application.

## System overview

microtun boot is the last link in a chain of signature checks rooted in each chip's mask ROM. It authenticates the application image, applies policy and the rollback floor where supported, performs any pending upgrade, and hands control to the application.

| Stage | ESP32 | RP2350 |
| --- | --- | --- |
| 1. Mask ROM | Verifies the ESP-IDF second stage (Secure Boot V2, RSA-3072); anchor: key digest in eFuse | Verifies microtun boot (secp256k1); anchor: BOOTKEY0 hash in OTP, burned on first boot |
| 2. Second stage | ESP-IDF second stage verifies microtun boot with the same Secure Boot V2 key; reads the unsigned partition table | None; the ROM verifies microtun boot directly |
| 3. microtun boot | Runs as the factory application; validates the image (P-256, policy, eFuse counter on C3 and C6, not C5), performs upgrades, re-authenticates before handoff | Runs from SRAM; validates the image (P-256, policy, OTP counter), performs upgrades, locks OTP pages 1, 2 and 32, re-authenticates before handoff |
| 4. Handoff | RAM loader in protected IRAM copies RAM segments and programs the MMU, re-reading flash after the final check | Maps the selected slot into the application window and jumps to the reset vector |
| 5. Application | Full privilege; can write flash and eFuses | Arm Secure; can write flash and any OTP page not locked at handoff |

On ESP32 the ROM's guarantee reaches microtun boot through the ESP-IDF second stage; on RP2350 the ROM checks microtun boot directly. Both chips execute the application from external QSPI flash.

| Trust anchor | Stored in | Protects |
| --- | --- | --- |
| ESP Secure Boot V2 RSA-3072 key digest | ESP32 eFuse | IDF second stage and microtun boot |
| RP2350 secp256k1 key hash (BOOTKEY0) | RP2350 OTP page 2 | microtun boot |
| P-256 key ring | Compiled into microtun boot only | Application images |
| Security counter | ESP32-C3 and ESP32-C6 eFuse `SECURE_VERSION` (16 epochs each); RP2350 OTP page 32 (504 epochs) | Rollback floor for application images |

microtun boot also consumes inputs that are **not** authenticated: slot trailers (upgrade requests, confirmation, swap progress), the image header before validation, and on ESP32 the partition table. Each limitation that follows from this is listed under Known limitations.

## Security objectives in scope

microtun boot commits to seven properties, subject to the chip-specific exceptions noted below. Each holds only under the attacker model and integrator responsibilities described later in this document.

| # | Objective | Mechanism | When enforced |
| --- | --- | --- | --- |
| O1 | Only authentic images execute | SHA-256 over header, body and protected TLVs; ECDSA P-256 signature checked against a compiled-in key ring, selected by `KEYHASH` or `PUBKEY` TLV | Before an image is installed, and again immediately before handoff |
| O2 | Signed metadata cannot be swapped | TLVs outside the signed area are allow-listed; `SEC_CNT`, VID and CID are accepted only from the protected area | During every image validation |
| O3 | Images are bound to a product class | Protected imgtool VID/CID UUIDs must match the board's image policy | During every image validation |
| O4 | Hardware rollback floor | Monotonic eFuse (ESP32-C3, ESP32-C6) or OTP (RP2350) counter; not available on ESP32-C5 (L8b); images below the floor are rejected; burned for selected permanent/confirmed images or for confirmations preserved before swap/overwrite; always after re-authentication | On every boot, before each counter burn, and at handoff |
| O5 | Malformed executables are rejected | ESP segment plan and Cortex-M vector checks confine RAM loads, MMU maps, stack and entry point | Before an image is installed, and again at handoff |
| O6 | Power-loss safety of upgrades | Restartable swap, overwrite and direct-XIP state machines; trailer destroyed back to front | During upgrades and on resume after power loss |
| O7 | Bootloader remains the only firmware execution authority | The application updater holds no trusted keys and can stage invalid or policy-rejected candidates; microtun boot independently authenticates signatures, policy and rollback counter (where supported) before installing or executing an image | At boot selection and again before handoff |

Version-based downgrade prevention (`downgrade-prevention` feature) is **not** listed as an objective. It protects only the legitimate update path; see Known limitations.

## Attacker model

microtun boot fully defends against attackers who can only supply image bytes; it partially defends against attackers who already run code on the device or hold it physically. No attacker in any class is assumed to hold a trusted signing key.

| Attacker | Assumed capability | microtun boot prevents | microtun boot does not prevent | Coverage |
| --- | --- | --- | --- | --- |
| A1 Remote image supplier | Delivers arbitrary bytes through the application's update transport | Booting unsigned or wrong-class images; executing images below the bootloader version or counter floor (where supported) | Aborting transfers, filling the inactive slot, flash wear through repeated erase cycles | In scope |
| A2 Compromised application | Code execution in the running app with full privilege (RISC-V M-mode, Arm Secure) and raw flash write | Persisting unsigned code across reset; booting below the hardware security counter floor (where supported); on RP2350, writing OTP pages 1, 2 and 32 | Reinstalling an **older signed** image whose `SEC_CNT` is at or above the floor (any signed epoch on C5); bricking the device; on ESP32, burning eFuses including `SECURE_VERSION` | Partial |
| A3 Physical, interfaces only | ROM download mode, BOOTSEL/PICOBOOT, or SWD/JTAG where not disabled; no probing of the flash bus | Same as A2 once the ROM secure-boot chain is enrolled and debug is disabled | Everything, while debug access remains enabled (dev profiles) | Partial |
| A4 Physical, flash bus | Flash interposer or emulator able to serve different data on successive reads | Nothing beyond A3 | Substituting code after validation (time-of-check/time-of-use) | Out of scope |
| A5 Fault injection | Voltage, clock, EM or laser glitches timed against the bootloader | Raises the number of precisely placed faults needed at the handoff and at counter burns | A determined, resynchronising attacker | Best effort |
| A6 Key holder | Access to a trusted P-256 signing key or ROM signing key | Nothing | Everything | Out of scope |

## Out of scope

The following are explicitly not provided by microtun boot and must be covered elsewhere if the product needs them.

- **Confidentiality.** Images are not encrypted; encrypted and compressed images are rejected. Flash contents, including application secrets, are readable by anyone with flash access.
- **Key management.** Generation, storage, rotation and revocation of P-256, ESP Secure Boot V2 RSA-3072 and RP2350 secp256k1 keys.
- **Transport security.** Authentication, encryption and rate limiting of the channel that delivers updates. The upload session API has no authentication of its own.
- **Runtime protection of the application.** Once the handoff completes, the application runs with full privilege; microtun boot offers no isolation, measurement or attestation after that point.
- **Physical flash-bus attacks.** Interposers and flash emulators (attacker A4).
- **Side channels.** Power and EM analysis of signature verification. Verification uses only public keys, so no secret is at stake in microtun boot itself.
- **Signature schemes other than ECDSA P-256 / SHA-256**, including RSA, Ed25519, `--pure` signatures, multi-image dependencies and RAM-load mode.

## Fault-injection hardening

microtun boot hardens only the two irreversible steps, the jump into the application and each security-counter burn. Every other check is a single software decision that one well-placed fault can flip.

At those two steps the image is authenticated a second time, independently of the boot decision, and the board re-derives what will run: the segment and MMU plan on ESP32, the XIP translation and vector table on RP2350. The rollback floor is read twice on boards with a hardware counter. The combined verdict is a 32-bit pattern consumed by a hardened check that evaluates it three times behind a TRNG-randomised delay of up to 4,096 loop iterations where supported. On ESP32-C5, the counter check and random delay are temporarily absent; independent authentication, load-plan agreement and the hardened verdict remain. Failure diverges into a repeated trap that resets the device.

Limits of this hardening:

- The compiler hints that keep redundant checks apart do not guarantee machine-code structure; the compiler can still merge or outline checks. Resistance must be measured with instruction-skip campaigns against the shipped binary.
- The random delay does not stop an attacker who resynchronises on the power or EM trace.
- There is no persistent lockout after repeated detections, so attempts are bounded only by time.
- Wrong rejections are not hardened. A fault that rejects a valid image fails closed (denial of service).
- Primary-slot rejection, which triggers a destructive bootstrap overwrite, is confirmed by a second validation; other erase decisions are not.

Hardware detectors complement the software:

| Board | Detector | Coverage | Can the application disable it? |
| --- | --- | --- | --- |
| ESP32-C3 | Clock glitch reset only | From IDF second stage onward; ROM unprotected | Yes, no lock exists |
| ESP32-C5 | Power (voltage) glitch reset only; the chip has no clock glitch detector | From IDF second stage onward; ROM unprotected | Yes, no lock exists |
| ESP32-C6 | None: ESP-IDF supports no clock or power glitch reset on this chip (L8c) | Software hardening only | Not applicable |
| RP2350 | Four power/clock glitch detectors, armed from OTP after enrollment | From power-on, including ROM signature check | No, configuration locked until reset |

## Known limitations

The hardware security counter is the only rollback control that survives an attacker with flash write access; everything version-based below it can be bypassed by attacker A2. The table lists every limitation identified, core first, then per board.

| ID | Applies to | Limitation | Consequence | Mitigation available |
| --- | --- | --- | --- | --- |
| L1 | Core, swap mode | A revert skips the version check and is selected purely from the primary trailer, which sits in its own sectors. A forged mid-swap status is resumed without any candidate check. | A2 can reinstall any older signed image and have it marked permanent | Bump `SEC_CNT` on every security-fix release where enforced (not C5) |
| L2 | Core, all modes | The downgrade reference version is read from the primary header, which is not authenticated at that point | A2 can erase the header to defeat the check | Same as L1 |
| L3 | Both boards | Images are validated, then executed from or re-read out of external flash | A4 can substitute code after validation | Flash encryption or in-package flash; physical protection |
| L4 | Both boards | No hardware anti-rollback for microtun boot itself. On ESP32-C3 and ESP32-C6 `SECURE_VERSION` is consumed by the application counter; IDF anti-rollback is disabled on every ESP chip; on RP2350 the ROM rollback version is unused | An older, vulnerable microtun boot signed with the same root key can be reflashed | Plan key revocation (ESP Secure Boot V2 digest slots) or ROM rollback versions before the first microtun boot fix ships |
| L5 | Both boards | When `MICROTUN_BOOT_PUBKEYS` is unset, builds trust the P-256 key `microtun-boot-local-p256` (or `MICROTUN_SIGNER_KEY_ID`) from microtun-signer's local store; without `MICROTUN_SIGNER_URL`, host tools sign with the local development keys. The keys are generated on each machine, outside the repository, **not committed**. A missing key makes the build or signing operation fail; with a signer service URL, the build requires an explicit `MICROTUN_BOOT_PUBKEYS`. | A release can accidentally trust or use a development key; exposure of its private half would allow unauthorized signed updates | Configure production public keys and signing credentials explicitly; verify every release key ring against approved production key fingerprints |
| L6 | ESP32 | Slot locations come from the unsigned partition table. Only flash below the table end is reserved, so slots may overlap the `microtun-boot`, `nvs`, `phy_init` or `storage` partitions | A corrupted or edited table turns microtun boot's erase and copy operations against its own partition or application data | Write-protect or verify the table at manufacture |
| L7 | ESP32 | Flash encryption is unsupported; the RAM loader reads segments with raw ROM flash reads; neither IDF profile disables ROM download mode | Flash is readable and writable over USB/UART; enabling encryption later requires loader changes | Manufacturing lockdown of download mode |
| L8 | ESP32-C3, ESP32-C6 | The counter has 16 epochs for the product's lifetime, and eFuses stay programmable by the application | A2 can exhaust the epoch budget or burn unrelated eFuses | Reserve epochs for security releases only |
| L8a | ESP32-C5 | esp-hal 1.2.2 has no TRNG entropy source; random delays are disabled rather than using a pseudo-RNG | No timing jitter before the hardened checks | Rely on the power glitch detector and physical protection; TODO: restore verified TRNG jitter |
| L8b | ESP32-C5 | Hardware security counter disabled due to the eFuse issue with esp-hal 1.2.2; `SEC_CNT` is not required or enforced | Older signed images can be reflashed regardless of epoch; O4 does not apply | Do not rely on C5 for hardware anti-rollback. TODO: verify the eFuse implementation on expendable hardware and restore policy, store, handoff floor check, boot info and HIL coverage |
| L8c | ESP32-C6 | The chip has no glitch detector that ESP-IDF supports, so no detector reset backs the software hardening; the eFuse counter and the RAM handoff's C6 register sequence are new and not yet covered by a hardware HIL run in CI | Fault injection meets only the hardened, TRNG-jittered checks; no reset on a detected glitch | Physical protection; run the HIL smoke and `security-counter` suites on an expendable board before relying on the port |
| L9 | RP2350 | Enrollment trusts whatever key signed the running microtun boot (trust on first use) and runs unconditionally; without `production` the default OTP policy keeps SWD and spare ROM key slots open; the host-tool acknowledgement is bypassed by flashing directly | A fresh board can be irreversibly locked to a locally generated development ROM key or to an unintended key | Build with `--features production` and provision only in a controlled environment using an approved ROM signing key |
| L10 | RP2350 | BOOTSEL/PICOBOOT stays enabled; OTP page locks last only until reset; OTP pages other than 1, 2 and 32 are unprotected | A3 retains flash access; other OTP data is writable by A2 | Burn permanent page locks and disable BOOTSEL as a manufacturing step |

## Integrator responsibilities

The objectives above hold in production only if every item below is done. Each one closes an assumption microtun boot makes but cannot enforce itself.

**All boards**

- [ ] Generate production P-256, Secure Boot V2 and ROM signing keys; keep private halves off build machines and out of the repository.
- [ ] Build with `MICROTUN_BOOT_PUBKEYS` set, and verify every release key ring contains only approved production public-key fingerprints, not a locally generated development key.
- [ ] Provision trusted signing keys and image policy only in the bootloader; verify that the application firmware does not embed a redundant signing-key ring.
- [ ] Increment `SEC_CNT` in every release that fixes a vulnerability on boards with hardware rollback. C5 cannot enforce it yet.
- [ ] Authenticate and rate-limit the update transport in the application.
- [ ] Decide how a microtun boot vulnerability would be remediated without hardware bootloader rollback (L4).

**ESP32**

- [ ] Use the `release` IDF profile so JTAG is disabled and unused Secure Boot V2 digest slots are revoked.
- [ ] Lock down ROM download mode at manufacture.
- [ ] Keep IDF `APP_ANTI_ROLLBACK` disabled; `SECURE_VERSION` belongs to microtun boot on C3 and C6 and is reserved for future support on C5.
- [ ] Verify the flashed partition table matches the board's reference partition layout at end of line.
- [ ] Budget the 16 available C3 and C6 rollback epochs across the product lifetime. C5 has no enforced epochs for now; revisit its 9-bit allocation when support returns.

**RP2350**

- [ ] Build with `--features production`, leaving default features enabled (`downgrade-prevention`, `bootstrap`, `uuid`), and provision only in a controlled environment. `production` adds hardened OTP lockdowns only; if using `--no-default-features`, explicitly re-enable any required policy features. Cargo `--release` alone still uses development OTP policy.
- [ ] Reserve OTP page 32 (rows `0x800`–`0x83f`) exclusively for the counter, raw mode.
- [ ] Disable BOOTSEL/PICOBOOT and burn permanent OTP page locks where the product allows it.
- [ ] Validate on a sacrificial board that the supply and clock never trip the glitch detectors before enrolling production units.
