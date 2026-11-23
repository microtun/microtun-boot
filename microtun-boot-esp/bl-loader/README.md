# bl-loader

The RAM handoff payload for ESP chips. Loading an application's RAM segments
overwrites the bootloader, so after validating the image microtun boot copies this
tiny, separately linked program into a protected IRAM region and lets it copy
the segments, map flash and jump to the application.

You do not build it directly: `microtun-boot-esp`'s `boot-runtime` feature builds
and embeds it. Because it is its own Cargo workspace, root `cargo fmt`,
`cargo clippy` and `cargo test` do not cover it.

Each chip has its own ROM entry points (`ld/<chip>.x.in`) and hardware
module (`hw` in `src/main.rs`): the ESP32-C3 maps flash through ROM MMU
setters; the ESP32-C5 writes its shared I/D MMU table through the SPI_MEM
registers, as ESP-IDF's `mmu_ll` does, and masks CLIC interrupt lines.

The payload must fit in the **4 KiB** protected IRAM region it is copied to.
Reduce code size rather than enlarging the region.
