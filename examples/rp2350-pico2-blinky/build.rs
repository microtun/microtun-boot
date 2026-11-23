fn main() {
    // Writes memory.x (execution window at 0x10400200, SRAM below the
    // boot-info record) followed by microtun boot's application contract
    // (`microtun-boot-rp.x`: boot-info RAM reservation, no ROM image definition),
    // then links cortex-m-rt's link.x. Do not add a memory.x of your own.
    microtun_boot_build::configure_rp2350_application(
        microtun_boot_layout_rp2350_pico2::MAX_IMAGE_SIZE,
    );
}
