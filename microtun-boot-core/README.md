# microtun-boot-core

The platform-independent core of microtun boot, a secure boot bootloader with
[MCUboot](https://github.com/mcu-tools/mcuboot) compatibility.

Images from `imgtool sign` work unchanged. Only ECDSA P-256 over SHA-256 is
supported; encrypted and compressed images are rejected.

This crate holds everything that does not care which chip it runs on: image
validation and policy, the boot decision for every upgrade strategy, and the
application-side updater.

## Using it

Most users should depend on a platform crate (`microtun-boot-esp`, `microtun-boot-rp`)
instead of this crate. To port microtun boot to a new platform, start from
`boards/qemu/microtun-boot/src/main.rs`, the smallest complete bootloader.

Applications never hold signing keys: the updater only stages images, and the
bootloader alone decides what runs.

## Updating from an application

The platform crate builds a `FirmwareUpdater` from the bootloader's handoff,
so the application does not choose slots or an upgrade mode itself. Bring your
own transport.

```rust,ignore
let boot = BootInfo::read().expect("bootloader handoff");
let mut updater = microtun_boot_esp::updater(&layout, &boot, &flash)?;

// Stage an image signed without `--pad`: padding would extend it into the
// trailer area the updater reserves for itself. Chunks can be any size.
let mut writer = updater.begin_update()?;
while let Some(chunk) = transport.next_chunk() {
    writer.write(chunk)?;
}
writer.finish()?;
updater.mark_for_test()?; // or mark_permanent()
// Reset. The bootloader authenticates the image before installing it.

// In the new image, after its own health checks:
if updater.needs_confirmation()? {
    updater.confirm()?;
}
```

A test image that resets before `confirm()` is reverted by the swap and
`direct-xip-revert` strategies. Until it confirms, the update slot holds the
fallback image, so you cannot start another update during an unconfirmed
trial. For transports that deliver numbered packets and may retry or stall,
use `upload::FirmwareUpload` instead of `begin_update`'s writer.

## Security counter

Every image carries a signed security counter, and the platform stores a
rollback floor in eFuse or OTP. An image below the floor is never booted. The
floor is raised once a new image is confirmed (or immediately for a permanent
install). Epochs are a finite hardware budget, so raise the counter only for
security fixes.

## Cargo features

| Feature | Default | Purpose |
|---|---|---|
| `uuid` | yes | Parse imgtool `--vid`/`--cid` names into UUIDs |
| `log` / `defmt` | no | Diagnostics; enable at most one |
