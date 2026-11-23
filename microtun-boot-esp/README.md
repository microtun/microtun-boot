# microtun-boot-esp

ESP chip support for microtun boot (currently ESP32-C3 only), used by both the
bootloader and applications that update themselves. Slot locations come from
the partition table at runtime, so nothing here is board-specific.

For a complete, flashable setup see the
[ESP32-C3 SuperMini board](../boards/esp32c3-supermini/README.md).
`tools/` holds the `microtun-boot-tools-esp` host tool.

## Using it from an application

```toml
[dependencies]
microtun-boot-esp = { workspace = true }   # the workspace selects `esp32c3`

[build-dependencies]
microtun-boot-build = { workspace = true }
```

```rust,ignore
// build.rs: links esp-hal's linkall.x, then microtun boot's application contract.
fn main() {
    microtun_boot_build::configure_esp_application();
}
```

```rust,ignore
let boot = microtun_boot_esp::BootInfo::read().expect("bootloader handoff");
let flash = microtun_boot_esp::open_flash(peripherals.FLASH);
let layout = microtun_boot_esp::read_layout(&flash).expect("partition table");
let mut updater = microtun_boot_esp::updater(&layout, &boot, &flash).expect("layout");
```

Then follow the [core README](../microtun-boot-core/README.md#updating-from-an-application)
to stage and confirm images, and the
[board README](../boards/esp32c3-supermini/README.md#writing-an-application)
for what else your application must do (watchdog, confirmation).

## Cargo features

| Feature | Default | Purpose |
|---|---|---|
| `esp32c3` | no | Chip selection; exactly one is required |
| `boot-runtime` | no | **Bootloader only:** start-up code and the [RAM handoff loader](bl-loader/README.md). Applications leave it off |
| `bl-loader-diagnostics` | no | Logging from the RAM handoff loader |
| `log` / `defmt` | `log` | Diagnostics |
| `uuid` | no | Parse imgtool `--vid`/`--cid` names into UUIDs |
