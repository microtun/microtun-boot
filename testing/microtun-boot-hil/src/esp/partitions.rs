//! Host-side snapshot of the exact partition table programmed into fixtures.
//! No partition offsets or sizes are compiled into the harness.

use esp_idf_part::PartitionTable;
use microtun_boot_core::{FlashBounds, Region, SlotLayout, UpgradeMode};

use super::{EspTarget, ToolsBoard};
use crate::{Error, Result, flash::Partition};

pub(super) struct Partitions {
    pub bytes: Vec<u8>,
    pub offset: u32,
    pub slots: SlotLayout,
    pub factory: Region,
    pub storage: Region,
}

impl Partitions {
    fn from_bytes(board: &ToolsBoard, bytes: Vec<u8>, offset: u32) -> Result<Self> {
        let chip = board.chip;
        let flash_size = board.flash_size as usize;
        let table = PartitionTable::try_from_bytes(bytes.clone()).map_err(Error::external)?;
        table.validate().map_err(Error::external)?;
        let reserved = offset
            .checked_add(chip.partition_table_size)
            .filter(|end| *end as usize <= flash_size)
            .ok_or("partition table extends past flash")?;
        // The table follows the ROM-loaded second stage.
        if offset <= chip.rom_bootloader_offset || !offset.is_multiple_of(chip.erase_size) {
            return Err("invalid partition table offset".into());
        }
        for entry in table.partitions() {
            let region = Region::new(entry.offset(), entry.size());
            region
                .check::<Partition>(flash_size)
                .map_err(Error::external)?;
            if region.offset < reserved {
                return Err("partition overlaps bootloader or partition table".into());
            }
        }
        let find = |name: &str| -> Result<Region> {
            let entry = table
                .find(name)
                .ok_or_else(|| format!("missing partition {name}"))?;
            Ok(Region::new(entry.offset(), entry.size()))
        };
        let slots = SlotLayout {
            primary: find("image-0")?,
            secondary: find("image-1")?,
            scratch: Some(find("image-scratch")?),
        };
        slots
            .validate::<Partition>(
                FlashBounds {
                    capacity: flash_size,
                    reserved,
                },
                UpgradeMode::SwapScratch,
            )
            .map_err(Error::external)?;
        let factory = find("microtun-boot")?;
        let storage = find("storage")?;
        Ok(Self {
            bytes,
            offset,
            slots,
            factory,
            storage,
        })
    }
}

/// Cache one table snapshot for signing, composition and flashing. This must
/// not refer to [`EspTarget::board`]: its signing configuration is
/// initialized from this table.
pub(super) fn read(target: &'static EspTarget) -> Result<&'static Partitions> {
    target
        .cache
        .partitions
        .get_or_init(|| load(target.tools))
        .as_ref()
        .map_err(Clone::clone)
}

fn load(board: &'static ToolsBoard) -> Result<Partitions> {
    let layout = microtun_boot_tools_esp::layout::default_layout(board).map_err(Error::external)?;
    let bytes = layout.binary().map_err(Error::external)?;
    Partitions::from_bytes(board, bytes, layout.offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esp::targets;

    fn binary(csv: &str) -> Vec<u8> {
        PartitionTable::try_from_str(csv).unwrap().to_bin().unwrap()
    }

    #[test]
    fn follows_relocated_and_resized_partitions_in_binary_table() {
        // Deliberately unlike any board's default CSV. Neither factory nor
        // storage placement, slot sizes or slot starts may be assumed.
        let bytes = binary(
            "microtun-boot,app,factory,0x30000,0x30000\n\
            image-0,data,undefined,0x7f000,0x100000\n\
            image-1,data,undefined,0x18f000,0x100000\n\
            image-scratch,data,undefined,0x290000,0x8000\n\
            storage,data,undefined,0x2a0000,0x10000\n",
        );
        for target in targets() {
            let table = Partitions::from_bytes(target.tools, bytes.clone(), 0x10000).unwrap();
            assert_eq!(table.bytes, bytes);
            assert_eq!(table.factory, Region::new(0x30000, 0x30000));
            assert_eq!(table.slots.primary, Region::new(0x7f000, 0x100000));
            assert_eq!(table.slots.secondary, Region::new(0x18f000, 0x100000));
            assert_eq!(table.slots.scratch, Some(Region::new(0x290000, 0x8000)));
            assert_eq!(table.storage, Region::new(0x2a0000, 0x10000));
        }
    }

    #[test]
    fn signing_and_composition_share_the_programmed_table() {
        for target in targets() {
            let table = read(target).unwrap();
            assert_eq!(target.board().signing.slot_size, table.slots.primary.size);
            let chain = super::super::artifacts::Bootloader {
                image: vec![0xa5; 64],
                idf_image: vec![0x5a; 64],
            };
            let flash = super::super::flash::compose(target, &chain).unwrap();
            let bytes = flash.bytes();
            assert_eq!(bytes.len(), target.tools.flash_size as usize);
            let offset = table.offset as usize;
            assert_eq!(
                &bytes[offset..offset + table.bytes.len()],
                table.bytes.as_slice()
            );
            let factory = table.factory.offset as usize;
            assert_eq!(
                &bytes[factory..factory + chain.image.len()],
                chain.image.as_slice()
            );
            assert_eq!(flash.layout, table.slots);
            assert_eq!(flash.storage, Some(table.storage));
        }
    }

    /// Slots, scratch and storage lie within the board's configured flash,
    /// and storage ends exactly at its end.
    #[test]
    fn programmed_table_fills_the_board_flash() {
        for target in targets() {
            let table = read(target).unwrap();
            let end = table.storage.offset + table.storage.size;
            assert_eq!(end, target.tools.flash_size, "{}", target.tools.name);
            assert!(table.offset > target.chip().rom_bootloader_offset);
        }
    }

    #[test]
    fn rejects_missing_slots_and_invalid_geometry() {
        let factory = "microtun-boot,app,factory,0x20000,0x40000\n";
        let unequal = format!(
            "{factory}\
            image-0,data,undefined,0x6f000,0x100000\n\
            image-1,data,undefined,0x17f000,0x110000\n\
            image-scratch,data,undefined,0x290000,0x8000\n\
            storage,data,undefined,0x2a0000,0x10000\n"
        );
        for target in targets() {
            let board = target.tools;
            assert!(Partitions::from_bytes(board, binary(factory), 0x10000).is_err());
            assert!(Partitions::from_bytes(board, binary(&unequal), 0x10000).is_err());
            assert!(Partitions::from_bytes(board, binary(factory), 0x20000).is_err());
            assert!(Partitions::from_bytes(board, vec![0; 32], 0x10000).is_err());
            // The table cannot start at or before the ROM-loaded second stage.
            let rom = board.chip.rom_bootloader_offset;
            assert!(Partitions::from_bytes(board, binary(factory), rom).is_err());
        }
    }

    /// A slot pair that extends beyond 4 MiB is rejected on the 4 MiB boards
    /// (C3, C5 N4) and accepted on the larger C5 variants: the check follows
    /// each board's configured limit.
    #[test]
    fn partitions_are_bounded_by_the_board_flash_size() {
        let large = binary(
            "microtun-boot,app,factory,0x20000,0x40000\n\
            image-0,data,undefined,0x6f000,0x300000\n\
            image-1,data,undefined,0x36f000,0x300000\n\
            image-scratch,data,undefined,0x670000,0x10000\n\
            storage,data,undefined,0x680000,0x8000\n",
        );
        for target in targets() {
            let fits = target.tools.flash_size >= 0x0068_8000;
            let parsed = Partitions::from_bytes(target.tools, large.clone(), 0x10000);
            assert_eq!(parsed.is_ok(), fits, "{}", target.tools.name);
        }
    }
}
