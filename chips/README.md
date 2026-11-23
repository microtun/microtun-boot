# Chip crates

Board-independent hardware and microtun boot ABI constants, one crate per chip,
shared by the platform crates, the board layout crates and the host tools. A
board's flash map does not live here: each `boards/<board>/layout` crate
combines these constants with the board's own partitions.

| Crate | Chip |
|---|---|
| `chips/esp32c3` | ESP32-C3 |
| `chips/esp32c5` | ESP32-C5 (rev v1.0 or later) |
| `chips/rp235x` | RP2350 family; one die covers RP2350A/B and RP2354A/B |
| `chips/qemu` | The patched QEMU `mps2-an385` test machine |
