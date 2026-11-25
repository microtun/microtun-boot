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
    /// The chip crate's named constants, for `layout --constant`. The
    /// board adds its own `FLASH_SIZE`.
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
            ]
        },
    }
};

pub static ESP32C5: ChipInfo = {
    use microtun_boot_layout_esp32c5_mini as c;
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
            use microtun_boot_layout_esp32c5_mini::*;
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
            ]
        },
    }
};

pub static ESP32C6: ChipInfo = {
    use microtun_boot_layout_esp32c6_supermini as c;
    ChipInfo {
        name: c::CHIP,
        display: "ESP32-C6",
        espflash: Chip::Esp32c6,
        xtal: XtalFrequency::_40Mhz,
        image_chip_id: 13,
        rom_bootloader_offset: c::ROM_BOOTLOADER_OFFSET,
        // 0x0..0x10000: the partition table follows at 0x10000 (IDF's Secure
        // Boot V2 limit, 64 KiB plus the signature sector, is not reached).
        max_stage2_signed: 0x10000,
        erase_size: c::ERASE_SIZE,
        header_size: c::HEADER_SIZE,
        mmu_page_size: c::MMU_PAGE_SIZE,
        partition_table_size: c::PARTITION_TABLE_SIZE,
        iram: (0x4080_0000, 0x4088_0000),
        iram_dram_offset: c::IRAM_DRAM_OFFSET,
        dram: (0x4080_0000, 0x4088_0000),
        rtc_fast: (c::RTC_FAST_BASE, c::RTC_FAST_BASE + c::RTC_FAST_SIZE),
        irom_base: 0x4200_0000,
        drom_base: 0x4200_0000,
        mmu_entries: 256,
        protected_start: c::PROTECTED_START,
        rom_data_start: c::ROM_DATA_START,
        boot_info_address: c::BOOT_INFO_ADDRESS,
        boot_info_size: c::BOOT_INFO_SIZE,
        constants: || {
            use microtun_boot_layout_esp32c6_supermini::*;
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
            ]
        },
    }
};

/// One supported board: its chip, configured flash capacity and boot-chain key.
#[derive(Debug)]
pub struct Board {
    /// The name `--board` and `MICROTUN_BOOT_ESP_BOARD` take.
    pub name: &'static str,
    /// Directory under `boards/` with the board's crates, Cargo config and
    /// ESP-IDF project. Boards that differ only in flash size share one.
    pub dir_name: &'static str,
    /// The board's ESP-IDF partition table, relative to [`Board::dir`].
    pub partitions: &'static str,
    pub chip: &'static ChipInfo,
    /// Flash capacity used by this board target (may be less than physical flash).
    pub flash_size: u32,
    /// Local development RSA-3072 Secure Boot V2 key (microtun-signer local store).
    pub dev_key: DevKey,
}

pub static ESP32C3_SUPERMINI: Board = Board {
    name: "esp32c3-supermini",
    dir_name: "esp32c3-supermini",
    partitions: "layout/partitions.csv",
    chip: &ESP32C3,
    flash_size: microtun_boot_layout_esp32c3_supermini::FLASH_SIZE,
    dev_key: DevKey {
        key_id: "microtun-boot-local-esp32c3-sbv2-rsa3072",
        env: "ESP_SBV2_KEY_ID",
    },
};

/// ESP32-C5 Mini: an ESP32-C5HF4 with 4 MiB of in-package flash.
pub static ESP32C5_MINI: Board = Board {
    name: "esp32c5-mini",
    dir_name: "esp32c5-mini",
    partitions: "layout/partitions.csv",
    chip: &ESP32C5,
    flash_size: microtun_boot_layout_esp32c5_mini::FLASH_SIZE,
    dev_key: DevKey {
        key_id: "microtun-boot-local-esp32c5-sbv2-rsa3072",
        env: "ESP_SBV2_KEY_ID",
    },
};

/// ESP32-C6 SuperMini: an ESP32-C6FH4 with 4 MiB of in-package flash.
pub static ESP32C6_SUPERMINI: Board = Board {
    name: "esp32c6-supermini",
    dir_name: "esp32c6-supermini",
    partitions: "layout/partitions.csv",
    chip: &ESP32C6,
    flash_size: microtun_boot_layout_esp32c6_supermini::FLASH_SIZE,
    dev_key: DevKey {
        key_id: "microtun-boot-local-esp32c6-sbv2-rsa3072",
        env: "ESP_SBV2_KEY_ID",
    },
};

/// Every supported board.
pub static BOARDS: [&Board; 3] = [&ESP32C3_SUPERMINI, &ESP32C5_MINI, &ESP32C6_SUPERMINI];

/// The names of every supported board.
pub const BOARD_NAMES: [&str; 3] = ["esp32c3-supermini", "esp32c5-mini", "esp32c6-supermini"];

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
    /// `boards/<dir_name>` in this repository.
    pub fn dir(&self) -> PathBuf {
        repo_root().join("boards").join(self.dir_name)
    }
    /// The board's partition table file.
    pub fn partitions_path(&self) -> PathBuf {
        self.dir().join(self.partitions)
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
    /// The flash size in ESP-IDF's `CONFIG_ESPTOOLPY_FLASHSIZE_<size>` form (`4MB`).
    pub fn idf_flash_size(&self) -> Result<String> {
        self.espflash_size()?;
        Ok(format!("{}MB", self.flash_size >> 20))
    }
    /// The flash-size field (upper nibble of byte 3) of this board's ESP
    /// image headers.
    pub fn header_flash_size(&self) -> Result<u8> {
        Ok(self.espflash_size()?.encode_flash_size()?)
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
            assert!(board.partitions_path().is_file(), "{}", board.name);
            board.espflash_size().unwrap();
        }
        assert!(Board::named("esp32c4").is_err());
    }

    #[test]
    fn boards_have_4_mib_of_flash() {
        for board in BOARDS {
            assert!(board.dir().is_dir(), "{}", board.name);
            assert_eq!(board.flash_size, 0x0040_0000, "{}", board.name);
            assert_eq!(board.idf_flash_size().unwrap(), "4MB");
            assert_eq!(board.header_flash_size().unwrap(), 2);
        }
        assert!(std::ptr::eq(ESP32C5_MINI.chip, &ESP32C5));
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
            // FLASH_SIZE belongs to the board, not the chip.
            assert!(!constants.contains_key("FLASH_SIZE"));
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
