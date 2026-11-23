//! CLI adapter for the shared ESP32 (C3, C5)/RP2350 HIL flash worker.
//! See `microtun_boot_hil::worker` for supported subcommands.

fn main() {
    microtun_boot_hil::worker::main()
}
