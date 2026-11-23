# microtun-boot-qemu

**Test only.** Platform support for the patched QEMU `mps2-an385` machine. It
plays the same role as `microtun-boot-esp` and `microtun-boot-rp`, letting the
regression suites run without hardware, and it is not a security boundary.

The QEMU board's bootloader and test firmware are its only users; if you build
your own for that machine, enable the `mps2-an385` feature. `tools/` holds the
`microtun-boot-tools-qemu` host tool.

See the [QEMU board README](../boards/qemu/README.md) for setup and usage.
`boards/qemu/microtun-boot/src/main.rs` is the smallest complete microtun boot
bootloader and a good starting point for a new platform.
