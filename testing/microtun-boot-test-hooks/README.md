# Firmware test hooks

The board test firmware (`boards/*/firmware`) reads a control record from its
`storage` partition that lets `microtun-boot-hil` make it confirm, reset, panic,
hang or erase itself for a given image version. This crate defines the record
format; each board's `main.rs` implements the actions.

> **Do not ship the board test firmware.** The record is unauthenticated:
> anything that can write the `storage` partition can trigger these actions,
> including destroying the active image. microtun boot still authenticates every
> image it boots.

With no valid record the firmware behaves normally. Otherwise it reports
progress on the console in `@fbt ...` lines, which the harness parses. The
record layout is defined in `src/lib.rs`; keep the `@fbt` lines identical in
every board's `main.rs`.

```sh
cargo test -p microtun-boot-test-hooks
```
