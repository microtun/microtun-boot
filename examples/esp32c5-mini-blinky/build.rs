fn main() {
    // Links esp-hal's linkall.x followed by microtun boot's application contract
    // (boot-info RAM reservation, handoff-loader keep-out).
    microtun_boot_build::configure_esp_application();
}
