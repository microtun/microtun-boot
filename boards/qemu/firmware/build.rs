use microtun_boot_layout_qemu as layout;

fn main() {
    microtun_boot_build::check_platform_chip("microtun-boot-qemu", layout::CHIP);
    let memory = format!(
        "MEMORY {{ FLASH : ORIGIN = {:#x}, LENGTH = {:#x}\n RAM : ORIGIN = {:#x}, LENGTH = {:#x} }}\n",
        layout::APP_VECTOR,
        layout::SLOT_SIZE - layout::HEADER_SIZE,
        layout::RAM_BASE,
        layout::BOOT_INFO_ADDRESS - layout::RAM_BASE
    );
    microtun_boot_build::write_out("memory.x", memory);
    println!(
        "cargo:rustc-link-search={}",
        microtun_boot_build::out_dir().display()
    );
    println!("cargo:rerun-if-changed=build.rs");
}
