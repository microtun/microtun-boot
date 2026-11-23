# microtun-boot-build

Host-side helpers for microtun boot. With no features it is a `build.rs` library
for bootloaders and applications; the `tools` and `cli` features add the
signing and image code shared by the host tools. Nothing here runs on a
target.

## In a build script

Add the crate under `[build-dependencies]` with no features (they only affect
the host tools), then call the helper for your binary from its own `build.rs`:

| Function | Use in |
|---|---|
| `configure_esp_application()` | ESP application or bootloader |
| `configure_rp2350_application(max_image_size)` | RP2350 application (generates `memory.x`) |
| `emit_public_keys()` | Bootloader only: embeds the trusted application keys |

```rust,ignore
// Application build.rs (ESP32-C3 or ESP32-C5)
fn main() {
    microtun_boot_build::configure_esp_application();
}
```

Applications never call `emit_public_keys`: only the bootloader verifies
signatures.

## Choosing trusted keys

`emit_public_keys` embeds the keys listed, comma-separated, in
`MICROTUN_BOOT_PUBKEYS`. Each file may be:

- a PEM public key, as printed by `microtun-signer public-key`
- DER
- a raw 65-byte point (`imgtool getpub -e raw`)

When `MICROTUN_BOOT_PUBKEYS` is unset, the bootloader trusts the same local key
that signs applications: `MICROTUN_BOOT_KEY_ID` if set, otherwise
`microtun-boot-local-p256`.
With a signer service configured, `MICROTUN_BOOT_PUBKEYS` is required: a build
never takes a key from the service. See [SIGNING.md](../SIGNING.md) for the
signer, its key types and the service setup.

## Cargo features

| Feature | Purpose |
|---|---|
| *(none)* | Build-script helpers only |
| `tools` | Image signing and assembly for the host tools (internal API) |
| `cli` | Shared command-line arguments; implies `tools` |
