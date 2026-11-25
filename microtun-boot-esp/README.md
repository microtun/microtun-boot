# microtun-boot-esp

ESP chip support for microtun boot (ESP32-C3, ESP32-C5 and ESP32-C6), used by both the
bootloader and applications that update themselves. Slot locations come from
the partition table at runtime, so nothing here is board-specific.

For a complete, flashable setup see the
[ESP32 board documentation](../boards/README-ESP32.md) and the
[board index](../boards/README.md).
`tools/` holds the `microtun-boot-tools-esp` host tool.

With esp-hal 1.2.2, ESP32-C5 temporarily has no hardware security-counter
support and no TRNG-backed jitter. The eFuse counter module and `BootRng`
are C3 and C6 only. C5 retains protected VID/CID policy, signature re-authentication,
load-plan verification and the power glitch detector. See the
[C5 board notes](../boards/esp32c5-mini/README.md#security-counter)
for restoration TODOs.

The ESP32-C6 has the C3's feature set (eFuse counter, TRNG jitter) except
glitch detection: ESP-IDF supports no glitch detector on it, so the `glitch`
module is not compiled for the C6.

## Using it from an application

```toml
[dependencies]
microtun-boot-esp = { workspace = true, features = ["esp32c3"] }   # or "esp32c5", "esp32c6"

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
[ESP32 board documentation](../boards/README-ESP32.md#writing-an-application)
for what else your application must do (watchdog, confirmation).

## Cargo features

| Feature | Default | Purpose |
|---|---|---|
| `esp32c3` | no | Chip selection (`riscv32imc-unknown-none-elf`); exactly one chip is required |
| `esp32c5` | no | Chip selection (`riscv32imac-unknown-none-elf`); its bootloader also needs `ESP_HAL_CONFIG_ENABLE_PMP=false` |
| `esp32c6` | no | Chip selection (`riscv32imac-unknown-none-elf`); its bootloader also needs `ESP_HAL_CONFIG_ENABLE_PMP=false` |
| `boot-runtime` | no | **Bootloader only:** start-up code and the [RAM handoff loader](bl-loader/README.md). Applications leave it off |
| `bl-loader-diagnostics` | no | Logging from the RAM handoff loader |
| `log` / `defmt` | `log` | Diagnostics |
| `uuid` | no | Parse imgtool `--vid`/`--cid` names into UUIDs |
