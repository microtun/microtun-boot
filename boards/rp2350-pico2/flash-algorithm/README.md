# RP2350 HIL flash algorithm

A probe-rs flash algorithm that `microtun-boot-hil` uses to restore a complete
4 MiB flash image quickly: the host sends LZ4-compressed sectors in one SWD
pass and the target writes only the sectors that changed. It is a test tool,
not a general flash target, and it is unauthenticated (which does not affect
the boot chain's signature checks).

## Rebuild

`target.yaml` embeds the compiled code and has no version check, so a stale
copy can corrupt the restore. After changing the sources, regenerate it and
commit it with them:

```sh
rustup target add thumbv8m.main-none-eabi
cd boards/rp2350-pico2/flash-algorithm
microtun-boot-tools-rp flash-algorithm
cd ../../..
cargo test -p microtun-boot-hil --features rp2350-pico2 --lib -- --test-threads=1
```

The build fails if the image grows past its SRAM limit; reduce code size
rather than bypassing the check.

## Wire format

This is the protocol reference. The target decoder (`src/protocol.rs`) and
the host encoder (`testing/microtun-boot-hil/src/flash_transport.rs`) must match
it and change together.

probe-rs sees a 5 MiB command window at `0x10000000`. `EraseSector` is a
no-op. Each `ProgramPage` carries one 4096-byte packet at
`0x10000000 + sequence * 4096`, with a little-endian header:

```text
0   sequence  u32   0, 1, 2, ...
4   length    u32   payload bytes (4084 for non-final packets)
8   last      u32   1 on the final packet, else 0
12  payload   [u8; length]; remaining bytes ignored
```

The concatenated payloads hold exactly 1024 records (one per 4 KiB sector,
in order), each a `u16` length followed by that many bytes. A record may span
packets:

| `length` | Sector contents |
|---|---|
| `0` | Erased |
| `4096` | Raw sector |
| `1..4095` | Raw LZ4 block that decodes to exactly 4096 bytes |

Error codes are the `Error` enum in `src/main.rs`; `255` means the algorithm
panicked.
