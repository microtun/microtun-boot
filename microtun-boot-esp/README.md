# microtun-boot-esp

ESP chip support for microtun boot (ESP32-C3 and ESP32-C5), used by both the
bootloader and applications that update themselves. Slot locations come from
the partition table at runtime, so nothing here is board-specific.

For a complete, flashable setup see the
[ESP32-C3 SuperMini](../boards/esp32c3-supermini/README.md) or
[ESP32-C5-DevKitC-1](../boards/esp32c5-devkitc-1/README.md) board.
`tools/` holds the `microtun-boot-tools-esp` host tool.

## Using it from an application

```toml
[dependencies]
microtun-boot-esp = { workspace = true, features = ["esp32c3"] }   # or "esp32c5"

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
board README ([C3](../boards/esp32c3-supermini/README.md#writing-an-application),
[C5](../boards/esp32c5-devkitc-1/README.md#writing-an-application))
for what else your application must do (watchdog, confirmation).

## Cargo features

| Feature | Default | Purpose |
|---|---|---|
| `esp32c3` | no | Chip selection (`riscv32imc-unknown-none-elf`); exactly one chip is required |
| `esp32c5` | no | Chip selection (`riscv32imac-unknown-none-elf`); its bootloader also needs `ESP_HAL_CONFIG_ENABLE_PMP=false` |
| `boot-runtime` | no | **Bootloader only:** start-up code and the [RAM handoff loader](bl-loader/README.md). Applications leave it off |
| `bl-loader-diagnostics` | no | Logging from the RAM handoff loader |
| `log` / `defmt` | `log` | Diagnostics |
| `uuid` | no | Parse imgtool `--vid`/`--cid` names into UUIDs |
