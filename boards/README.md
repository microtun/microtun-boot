# Boards

Bootloader, test firmware and flash layout for each supported board. Each
board README covers what is specific to that board; the boot chain and
workflow shared by the three ESP32 boards are in
[README-ESP32.md](README-ESP32.md).

| Board | Chip | Rust target | Host tool | Status |
|---|---|---|---|---|
| [ESP32-C3-MINI-1](esp32-c3-mini-1/README.md) | ESP32-C3, 4 MiB flash | `riscv32imc-unknown-none-elf` | `microtun-boot-tools-esp` | — |
| [ESP32-C5-MINI-1](esp32-c5-mini-1/README.md) | ESP32-C5HF4, 4 MiB in-package | `riscv32imac-unknown-none-elf` | `microtun-boot-tools-esp` | Host checks pass; run the HIL suite before relying on it |
| [ESP32-C6-MINI-1](esp32-c6-mini-1/README.md) | ESP32-C6FH4, 4 MiB in-package | `riscv32imac-unknown-none-elf` | `microtun-boot-tools-esp` | Host checks pass; RAM handoff and eFuse counter not yet run on hardware |
| [RP2350 Pico 2 / Pico 2 W](rp2350-pico2/README.md) | RP2350A, 4 MiB flash | `thumbv8m.main-none-eabihf` | `microtun-boot-tools-rp` | — |
| [QEMU](qemu/README.md) | Cortex-M3 (patched `mps2-an385`) | `thumbv7m-none-eabi` | `microtun-boot-tools-qemu` | Test target only, not a real board |

The ESP32 boards flash and monitor over the chip's native USB-Serial-JTAG
port; the RP2350 flashes over SWD and logs on UART0. Standalone application
examples live in [`examples/`](../examples/README.md).
