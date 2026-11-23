//! The third stage links like an ESP application; microtun-boot-esp's
//! `boot-runtime` feature builds and embeds the RAM handoff loader.
fn main() {
    microtun_boot_build::configure_esp_application();
    microtun_boot_build::emit_public_keys();
}
