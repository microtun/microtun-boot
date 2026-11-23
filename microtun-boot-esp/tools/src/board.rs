//! The ESP boards this tool supports and the chip facts it checks against.
//!
//! Chip values shared with the target come from the `chips/<chip>` crates
//! (through each board's layout crate); the memory windows mirror
//! `microtun-boot-esp/src/chip/<chip>.rs`.
use anyhow::{Result, bail};
use espflash::{
    flasher::FlashSize,
    target::{Chip, XtalFrequency},
};
use microtun_boot_build::tools::{repo_root, signer::DevKey};
use std::{collections::BTreeMap, path::PathBuf};

/// Selects the board when the `--board` option is absent. One of the two is
/// required: no board is assumed.
pub const BOARD_ENV: &str = "MICROTUN_BOOT_ESP_BOARD";

/// Facts about one ESP chip.
#[derive(Debug)]
pub struct ChipInfo {
    /// Cargo feature / MCUboot `--cid` name, e.g. `esp32c3`.
    pub name: &'static str,
    pub display: &'static str,
    pub espflash: Chip,
    pub xtal: XtalFrequency,
    /// ESP image header chip ID.
    pub image_chip_id: u16,
    /// Flash offset at which the mask ROM loads the second stage.
    pub rom_bootloader_offset: u32,
    /// Largest signed second stage (image plus 4 KiB signature sector).
    pub max_stage2_signed: u32,
    pub erase_size: u32,
    pub header_size: u32,
    pub mmu_page_size: u32,
    pub partition_table_size: u32,
    /// Instruction RAM window and its distance above the data alias.
    pub iram: (u32, u32),
    pub iram_dram_offset: u32,
    pub dram: (u32, u32),
    pub rtc_fast: (u32, u32),
    /// Flash cache windows (identical when I/D share one window).
    pub irom_base: u32,
    pub drom_base: u32,
    pub mmu_entries: u32,
    pub protected_start: u32,
    pub rom_data_start: u32,
    pub boot_info_address: u32,
    pub boot_info_size: u32,
    /// The chip crate's named constants, for `layout --constant`.
    pub constants: fn() -> BTreeMap<String, u32>,
}

impl ChipInfo {
    /// Size of one flash cache window.
    pub fn bus_size(&self) -> u32 {
        self.mmu_entries * self.mmu_page_size
    }
}

pub static ESP32C3: ChipInfo = {
    use microtun_boot_layout_esp32c3_supermini as c;
    ChipInfo {
        name: c::CHIP,
        display: "ESP32-C3",
        espflash: Chip::Esp32c3,
        xtal: XtalFrequency::_40Mhz,
        image_chip_id: 5,
        rom_bootloader_offset: c::ROM_BOOTLOADER_OFFSET,
        max_stage2_signed: 0xc000,
        erase_size: c::ERASE_SIZE,
        header_size: c::HEADER_SIZE,
        mmu_page_size: c::MMU_PAGE_SIZE,
        partition_table_size: c::PARTITION_TABLE_SIZE,
        iram: (0x4038_0000, 0x403e_0000),
        iram_dram_offset: c::IRAM_DRAM_OFFSET,
        dram: (0x3fc8_0000, 0x3fce_0000),
        rtc_fast: (c::RTC_FAST_BASE, c::RTC_FAST_BASE + c::RTC_FAST_SIZE),
        irom_base: 0x4200_0000,
        drom_base: 0x3c00_0000,
        mmu_entries: 128,
        protected_start: c::PROTECTED_START,
        rom_data_start: c::ROM_DATA_START,
        boot_info_address: c::BOOT_INFO_ADDRESS,
        boot_info_size: c::BOOT_INFO_SIZE,
        constants: || {
            use microtun_boot_layout_esp32c3_supermini::*;
            microtun_boot_build::named_constants![
                ROM_BOOTLOADER_OFFSET,
                ERASE_SIZE,
                HEADER_SIZE,
                MMU_PAGE_SIZE,
                PARTITION_TABLE_SIZE,
                IRAM_DRAM_OFFSET,
                BL_DRAM_BASE,
                PROTECTED_START,
                IRAM_HANDOFF_SIZE,
                IDF_IRAM_LOADER_START,
                BOOT_INFO_ADDRESS,
                BOOT_INFO_SIZE,
                ROM_DATA_START,
                RTC_FAST_BASE,
                RTC_FAST_SIZE,
                LOADER_PLAN_ADDRESS,
                LOADER_STACK_BOTTOM,
                LOADER_STACK_TOP,
                FLASH_SIZE,
            ]
        },
    }
};

pub static ESP32C5: ChipInfo = {
    use microtun_boot_layout_esp32c5_devkitc_1 as c;
    ChipInfo {
        name: c::CHIP,
        display: "ESP32-C5",
        espflash: Chip::Esp32c5,
        xtal: XtalFrequency::_48Mhz,
        image_chip_id: 23,
        rom_bootloader_offset: c::ROM_BOOTLOADER_OFFSET,
        // 0x2000..0x10000: the partition table follows at 0x10000.
        max_stage2_signed: 0xe000,
        erase_size: c::ERASE_SIZE,
        header_size: c::HEADER_SIZE,
        mmu_page_size: c::MMU_PAGE_SIZE,
        partition_table_size: c::PARTITION_TABLE_SIZE,
        iram: (0x4080_0000, 0x4086_0000),
        iram_dram_offset: c::IRAM_DRAM_OFFSET,
        dram: (0x4080_0000, 0x4086_0000),
        rtc_fast: (c::RTC_FAST_BASE, c::RTC_FAST_BASE + c::RTC_FAST_SIZE),
        irom_base: 0x4200_0000,
        drom_base: 0x4200_0000,
        mmu_entries: 512,
        protected_start: c::PROTECTED_START,
        rom_data_start: c::ROM_DATA_START,
        boot_info_address: c::BOOT_INFO_ADDRESS,
        boot_info_size: c::BOOT_INFO_SIZE,
        constants: || {
            use microtun_boot_layout_esp32c5_devkitc_1::*;
            microtun_boot_build::named_constants![
                ROM_BOOTLOADER_OFFSET,
                ERASE_SIZE,
                HEADER_SIZE,
                MMU_PAGE_SIZE,
                PARTITION_TABLE_SIZE,
                IRAM_DRAM_OFFSET,
                BL_DRAM_BASE,
                PROTECTED_START,
                IRAM_HANDOFF_SIZE,
                IDF_IRAM_LOADER_START,
                BOOT_INFO_ADDRESS,
                BOOT_INFO_SIZE,
                ROM_DATA_START,
                RTC_FAST_BASE,
                RTC_FAST_SIZE,
                LOADER_PLAN_ADDRESS,
                LOADER_STACK_BOTTOM,
                LOADER_STACK_TOP,
                FLASH_SIZE,
            ]
        },
    }
};

/// One supported board: its chip, installed flash and boot-chain key.
#[derive(Debug)]
pub struct Board {
    /// Directory name under `boards/`.
    pub name: &'static str,
    pub chip: &'static ChipInfo,
    /// Capacity of the flash installed on this board.
    pub flash_size: u32,
    /// Local development RSA-3072 Secure Boot V2 key (microtun-signer local store).
    pub dev_key: DevKey,
}

pub static ESP32C3_SUPERMINI: Board = Board {
    name: "esp32c3-supermini",
    chip: &ESP32C3,
    flash_size: microtun_boot_layout_esp32c3_supermini::FLASH_SIZE,
    dev_key: DevKey {
        key_id: "microtun-boot-local-esp32c3-sbv2-rsa3072",
        env: "ESP_SBV2_KEY_ID",
    },
};

pub static ESP32C5_DEVKITC_1: Board = Board {
    name: "esp32c5-devkitc-1",
    chip: &ESP32C5,
    flash_size: microtun_boot_layout_esp32c5_devkitc_1::FLASH_SIZE,
    dev_key: DevKey {
        key_id: "microtun-boot-local-esp32c5-sbv2-rsa3072",
        env: "ESP_SBV2_KEY_ID",
    },
};

/// Every supported board.
pub static BOARDS: [&Board; 2] = [&ESP32C3_SUPERMINI, &ESP32C5_DEVKITC_1];

/// The names of every supported board.
pub const BOARD_NAMES: [&str; 2] = ["esp32c3-supermini", "esp32c5-devkitc-1"];

impl Board {
    /// The board called `name`.
    pub fn named(name: &str) -> Result<&'static Board> {
        match BOARDS.iter().find(|board| board.name == name) {
            Some(board) => Ok(board),
            None => bail!(
                "unknown ESP board {name:?}; expected one of {}",
                BOARD_NAMES.join(", ")
            ),
        }
    }
    /// `boards/<name>` in this repository.
    pub fn dir(&self) -> PathBuf {
        repo_root().join("boards").join(self.name)
    }
    /// The `espflash` flash size setting for this board.
    pub fn espflash_size(&self) -> Result<FlashSize> {
        Ok(match self.flash_size {
            0x0040_0000 => FlashSize::_4Mb,
            0x0080_0000 => FlashSize::_8Mb,
            0x0100_0000 => FlashSize::_16Mb,
            size => bail!("unsupported flash size {size:#x}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_match_and_resolve() {
        for (board, name) in BOARDS.iter().zip(BOARD_NAMES) {
            assert_eq!(board.name, name);
            assert!(std::ptr::eq(Board::named(name).unwrap(), *board));
            assert!(board.dir().join("layout/partitions.csv").is_file());
            board.espflash_size().unwrap();
        }
        assert!(Board::named("esp32c4").is_err());
    }

    #[test]
    fn chip_facts_agree_with_espflash_and_chip_crates() {
        for board in BOARDS {
            let chip = board.chip;
            assert_eq!(chip.espflash.id(), chip.image_chip_id, "{}", chip.name);
            assert_eq!(chip.espflash.boot_address(), chip.rom_bootloader_offset);
            assert_eq!(chip.espflash.default_xtal_frequency(), chip.xtal);
            let constants = (chip.constants)();
            assert_eq!(constants["PROTECTED_START"], chip.protected_start);
            assert_eq!(constants["FLASH_SIZE"], board.flash_size);
            // The second stage ends before the partition table can start.
            assert!(chip.rom_bootloader_offset + chip.max_stage2_signed <= 0x10000);
            assert!(chip.protected_start < chip.rom_data_start);
            assert_eq!(
                chip.boot_info_address + chip.boot_info_size,
                chip.rom_data_start
            );
        }
    }
}
