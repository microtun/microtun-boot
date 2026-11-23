# Patched QEMU for the test board

Stock `mps2-an385` has no SPI flash, eFuse or RNG. The patch
`0001-an385-microtun-boot-test-peripherals.patch` adds **test-only** versions of
all three to QEMU v10.1.0. The register map is in the patch itself.

## Build with Docker

From the repository root, with Docker BuildKit:

```sh
mkdir -p target/qemu
docker build -f boards/qemu/qemu/Dockerfile.qemu \
  --output type=local,dest=target/qemu boards/qemu/qemu
target/qemu/qemu-system-arm --version
```

The result is a dynamically linked Debian trixie binary. Run it on a Linux
host of the same architecture, with glibc 2.41+ and QEMU's runtime libraries
(glib, pixman, zlib, libfdt). On macOS or Windows, run it and the tests in a
Linux VM or container.

## Build manually

```sh
git clone --branch v10.1.0 https://gitlab.com/qemu-project/qemu.git
cd qemu
git apply /path/to/microtun-boot/boards/qemu/qemu/0001-an385-microtun-boot-test-peripherals.patch
./configure --target-list=arm-softmmu --disable-docs
ninja -C build qemu-system-arm
```

Point `MICROTUN_BOOT_QEMU_BIN` at `build/qemu-system-arm`.
