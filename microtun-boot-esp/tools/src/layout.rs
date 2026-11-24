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
        values.insert("FLASH_SIZE".into(), self.board.flash_size);
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

/// The board's own partition table ([`Board::partitions_path`]).
pub fn default_layout(board: &'static Board) -> Result<EspLayout> {
    EspLayout::load(board, &board.partitions_path())
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

    /// The C5 variants share everything below `image-0`; slots, scratch and
    /// storage scale with flash, as documented in each partition table.
    #[test]
    fn c5_layouts_match_documentation() {
        use crate::board::{ESP32C5_DEVKITC_1_N4, ESP32C5_DEVKITC_1_N8, ESP32C5_DEVKITC_1_N16};
        for (board, flash, secondary, slot) in [
            (&ESP32C5_DEVKITC_1_N4, 0x0040_0000, 0x22f000, 0x1b9000),
            (&ESP32C5_DEVKITC_1_N8, 0x0080_0000, 0x42f000, 0x3b9000),
            (&ESP32C5_DEVKITC_1_N16, 0x0100_0000, 0x82f000, 0x7b9000),
        ] {
            let layout = default_layout(board).unwrap();
            assert_eq!(layout.board.flash_size, flash);
            assert_eq!(layout.offset, 0x10000);
            assert_eq!(layout.region("nvs").unwrap(), (0x11000, 0x5000));
            assert_eq!(layout.region("phy_init").unwrap(), (0x16000, 0x1000));
            assert_eq!(layout.region("microtun-boot").unwrap(), (0x20000, 0x40000));
            assert_eq!(layout.region("image-0").unwrap(), (0x6f000, slot));
            assert_eq!(layout.region("image-1").unwrap(), (secondary, slot));
            // A 28 KiB alignment gap separates the slots.
            assert_eq!(secondary - (0x6f000 + slot), 0x7000, "{}", board.name);
            let scratch = flash - 0x18000;
            assert_eq!(layout.region("image-scratch").unwrap(), (scratch, 0x10000));
            assert_eq!(secondary + slot, scratch, "{}", board.name);
            // storage is the final 32 KiB.
            assert_eq!(layout.region("storage").unwrap(), (flash - 0x8000, 0x8000));
            let constants = layout.constants().unwrap();
            assert_eq!(constants["BOOTLOADER_SIZE"], 0xe000);
            assert_eq!(constants["FLASH_SIZE"], flash);
        }
    }

    /// A larger variant's table does not fit a smaller board.
    #[test]
    fn c5_tables_are_rejected_on_smaller_variants() {
        use crate::board::{ESP32C5_DEVKITC_1_N4, ESP32C5_DEVKITC_1_N8, ESP32C5_DEVKITC_1_N16};
        let n8 = ESP32C5_DEVKITC_1_N8.partitions_path();
        let n16 = ESP32C5_DEVKITC_1_N16.partitions_path();
        assert!(EspLayout::load(&ESP32C5_DEVKITC_1_N4, &n8).is_err());
        assert!(EspLayout::load(&ESP32C5_DEVKITC_1_N4, &n16).is_err());
        assert!(EspLayout::load(&ESP32C5_DEVKITC_1_N8, &n16).is_err());
        // A smaller table is valid on a larger board; it just leaves flash unused.
        let n4 = ESP32C5_DEVKITC_1_N4.partitions_path();
        assert!(EspLayout::load(&ESP32C5_DEVKITC_1_N16, &n4).is_ok());
    }
}
