//! ESP-IDF partition table validation. ESP partitions remain a runtime input
//! shared with ESP-IDF (unlike RP2350, whose layout is compiled in).
use crate::board::Board;
use anyhow::{Context, Result, ensure};
use esp_idf_part::{AppType, DataType, PartitionTable, SubType, Type};
use microtun_boot_build::tools::number;
use std::{collections::BTreeMap, path::Path};

/// Partition table offset: environment first, then the board Cargo config.
pub fn table_offset(board: &Board) -> Result<u32> {
    const KEY: &str = "ESP_BOOTLOADER_ESP_IDF_CONFIG_PARTITION_TABLE_OFFSET";
    if let Ok(value) = std::env::var(KEY) {
        return number(&value);
    }
    let source = std::fs::read_to_string(board.dir().join(".cargo/config.toml"))?;
    let config: toml::Value = toml::from_str(&source)?;
    number(
        config["env"][KEY]
            .as_str()
            .context("missing ESP partition table offset")?,
    )
}

pub struct EspLayout {
    pub table: PartitionTable,
    pub offset: u32,
    pub board: &'static Board,
}

impl EspLayout {
    pub fn load(board: &'static Board, path: &Path) -> Result<Self> {
        let source = std::fs::read_to_string(path)?;
        Self::parse(board, &source, table_offset(board)?)
    }
    pub fn parse(board: &'static Board, source: &str, offset: u32) -> Result<Self> {
        let chip = board.chip;
        ensure!(
            offset > chip.rom_bootloader_offset
                && offset.is_multiple_of(chip.erase_size)
                && u64::from(offset) + u64::from(chip.partition_table_size)
                    <= u64::from(board.flash_size),
            "invalid partition table offset"
        );
        // Require explicit placement; upstream also accepts implicit offsets.
        for line in source.lines() {
            let line = line.split('#').next().unwrap().trim();
            if line.is_empty() {
                continue;
            }
            let fields: Vec<_> = line.split(',').map(str::trim).collect();
            ensure!(
                fields.len() == 5 && fields.iter().all(|s| !s.is_empty()),
                "expected five explicit partition fields: {line}"
            );
            ensure!(fields[0].len() <= 16, "partition name exceeds 16 bytes");
        }
        let table = PartitionTable::try_from_str(source)?;
        table.validate()?;
        let layout = Self {
            table,
            offset,
            board,
        };
        let mut occupied = Vec::new();
        for entry in layout.table.partitions() {
            let (start, size) = (entry.offset(), entry.size());
            let end = start.checked_add(size).context("partition overflow")?;
            ensure!(
                size > 0
                    && start.is_multiple_of(chip.erase_size)
                    && size.is_multiple_of(chip.erase_size),
                "{}: invalid alignment",
                entry.name()
            );
            ensure!(
                start >= offset + chip.partition_table_size && end <= board.flash_size,
                "{}: outside flash or overlaps table",
                entry.name()
            );
            for &(ref name, a, b) in &occupied {
                ensure!(
                    entry.name() != *name && (end <= a || b <= start),
                    "{}: duplicate or overlaps {name}",
                    entry.name()
                );
            }
            occupied.push((entry.name(), start, end));
        }
        let factory = layout
            .table
            .find("microtun-boot")
            .context("missing microtun-boot partition")?;
        ensure!(
            factory.ty() == Type::App && factory.subtype() == SubType::App(AppType::Factory),
            "microtun-boot must be app,factory"
        );
        ensure!(
            factory.offset().is_multiple_of(chip.mmu_page_size)
                && factory.size().is_multiple_of(chip.mmu_page_size),
            "factory partition must be MMU aligned"
        );
        for name in ["image-0", "image-1", "image-scratch", "storage"] {
            let p = layout
                .table
                .find(name)
                .with_context(|| format!("missing {name}"))?;
            ensure!(
                p.ty() == Type::Data && p.subtype() == SubType::Data(DataType::Undefined),
                "{name}: expected data,undefined"
            );
        }
        let primary = layout.region("image-0")?;
        let secondary = layout.region("image-1")?;
        ensure!(
            primary.1 == secondary.1,
            "image slots must have equal sizes"
        );
        ensure!(
            [primary.0, secondary.0]
                .iter()
                .all(|v| (v + chip.header_size).is_multiple_of(chip.mmu_page_size)),
            "slot payloads must be MMU aligned"
        );
        ensure!(
            layout.region("image-scratch")?.1 >= 2 * chip.erase_size,
            "scratch must hold two sectors"
        );
        ensure!(layout.region("storage")?.1 >= 32768, "storage needs 32 KiB");
        Ok(layout)
    }
    pub fn region(&self, name: &str) -> Result<(u32, u32)> {
        let p = self
            .table
            .find(name)
            .with_context(|| format!("missing partition {name}"))?;
        Ok((p.offset(), p.size()))
    }
    pub fn binary(&self) -> Result<Vec<u8>> {
        Ok(self.table.to_bin()?)
    }
    /// Chip and board constants plus the regions of this table.
    pub fn constants(&self) -> Result<BTreeMap<String, u32>> {
        // Chip (chips/<chip>) and board (FLASH_SIZE) constants.
        let mut values = (self.board.chip.constants)();
        values.insert("PARTITION_TABLE_OFFSET".into(), self.offset);
        values.insert(
            "BOOTLOADER_SIZE".into(),
            self.offset - self.board.chip.rom_bootloader_offset,
        );
        for (name, stem) in [
            ("microtun-boot", "MICROTUN_BOOT"),
            ("image-0", "PRIMARY"),
            ("image-1", "SECONDARY"),
            ("image-scratch", "SCRATCH"),
            ("storage", "STORAGE"),
        ] {
            let (offset, size) = self.region(name)?;
            values.insert(format!("{stem}_OFFSET"), offset);
            values.insert(format!("{stem}_SIZE"), size);
        }
        values.insert("SLOT_SIZE".into(), self.region("image-0")?.1);
        Ok(values)
    }
}

/// The board's `boards/<board>/layout/partitions.csv`.
pub fn default_layout(board: &'static Board) -> Result<EspLayout> {
    EspLayout::load(board, &board.dir().join("layout/partitions.csv"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::BOARDS;

    #[test]
    fn every_board_layout_is_valid() {
        for board in BOARDS {
            let layout = default_layout(board).unwrap();
            let constants = layout.constants().unwrap();
            assert_eq!(constants["FLASH_SIZE"], board.flash_size);
            let (factory, _) = layout.region("microtun-boot").unwrap();
            assert!(factory >= layout.offset + board.chip.partition_table_size);
            assert!(
                board.chip.rom_bootloader_offset + board.chip.max_stage2_signed <= layout.offset,
                "{}: second stage window overlaps the partition table",
                board.name
            );
        }
    }

    #[test]
    fn c5_layout_matches_documentation() {
        let layout = default_layout(&crate::board::ESP32C5_DEVKITC_1).unwrap();
        assert_eq!(layout.board.flash_size, 0x0040_0000);
        assert_eq!(layout.offset, 0x10000);
        assert_eq!(layout.region("image-0").unwrap(), (0x6f000, 0x1b9000));
        assert_eq!(layout.region("image-1").unwrap(), (0x22f000, 0x1b9000));
        assert_eq!(layout.region("image-scratch").unwrap(), (0x3e8000, 0x10000));
        assert_eq!(layout.region("storage").unwrap(), (0x3f8000, 0x8000));
        assert_eq!(layout.constants().unwrap()["BOOTLOADER_SIZE"], 0xe000);
    }
}
