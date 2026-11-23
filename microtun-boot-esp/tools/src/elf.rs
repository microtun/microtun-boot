//! ESP32 microtun boot ELF checks before ROM image conversion.
use crate::board::ChipInfo;
use anyhow::{Context, Result, ensure};
use microtun_boot_build::tools::elf::{
    abi::{EM_RISCV, PT_LOAD},
    parse, sections,
};

pub fn check_bootloader(data: &[u8], chip: &ChipInfo) -> Result<()> {
    let file = parse(data, EM_RISCV)?;
    for (name, _) in sections(&file)? {
        ensure!(name != ".bl_loader", "obsolete .bl_loader section");
    }
    let segments = file.segments().context("missing ESP program headers")?;
    ensure!(!segments.is_empty(), "missing ESP program headers");
    for p in segments {
        if p.p_type != PT_LOAD {
            continue;
        }
        ensure!(p.p_filesz <= p.p_memsz, "invalid ESP load sizes");
        file.segment_data(&p)?;
        let mut addr = p.p_vaddr;
        if (u64::from(chip.iram.0)..u64::from(chip.iram.1)).contains(&addr) {
            addr -= u64::from(chip.iram_dram_offset);
        }
        if addr < u64::from(chip.rom_data_start)
            && u64::from(chip.protected_start) < addr + p.p_memsz
        {
            ensure!(
                p.p_filesz == 0
                    && addr == u64::from(chip.boot_info_address)
                    && p.p_memsz == u64::from(chip.boot_info_size),
                "ESP load overlaps protected SRAM"
            );
        }
    }
    Ok(())
}
