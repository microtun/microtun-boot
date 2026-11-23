# microtun-boot-rp

RP2350 support for microtun boot, used by both the bootloader and applications
that update themselves. A board passes its flash map to `Layout::new`, which
checks it at compile time, so nothing here is board-specific.

For a complete, flashable setup see the
[RP2350 Pico 2 board](../boards/rp2350-pico2/README.md).
`tools/` holds the `microtun-boot-tools-rp` host tool.

## Using it from an application

```toml
[dependencies]
microtun-boot-rp = { workspace = true }   # the workspace selects `rp235xa`
microtun-boot-layout-rp2350-pico2 = { workspace = true }
cortex-m-rt = { workspace = true }
embassy-rp = { workspace = true, features = ["imagedef-none"] }

[build-dependencies]
microtun-boot-build = { workspace = true }
microtun-boot-layout-rp2350-pico2 = { workspace = true }
```

```rust,ignore
// build.rs: generates memory.x, so do not provide your own.
fn main() {
    microtun_boot_build::configure_rp2350_application(
        microtun_boot_layout_rp2350_pico2::MAX_IMAGE_SIZE,
    );
}
```

```rust,ignore
const LAYOUT: microtun_boot_rp::Layout =
    microtun_boot_rp::Layout::new(microtun_boot_layout_rp2350_pico2::FLASH_MAP);

let boot = microtun_boot_rp::BootInfo::read().expect("bootloader handoff");
let flash = microtun_boot_rp::share(
    microtun_boot_rp::hardware::flash::<{ LAYOUT.flash_size() }>(peripherals.FLASH),
);
let mut updater = microtun_boot_rp::updater(&LAYOUT, &boot, &flash).expect("layout");
```

Then follow the [core README](../microtun-boot-core/README.md#updating-from-an-application)
to stage and confirm images, and the
[board README](../boards/rp2350-pico2/README.md#writing-an-application)
for what else your application must do (watchdog, interrupts, core 1).

## Cargo features

None are enabled by default; the workspace dependency already selects
`rp235xa`, so you only pick a variant if you use this crate outside the
workspace.

| Feature | Purpose |
|---|---|
| `rp235xa`, `rp235xb` | Package variant; exactly one is required. The RP2354A/B parts are the same die with in-package flash and use the same features |
| `uart-log` | `log` output over a polling UART0 driver |
| `log` / `defmt` | Diagnostics |
| `uuid` | Parse imgtool `--vid`/`--cid` names into UUIDs |
