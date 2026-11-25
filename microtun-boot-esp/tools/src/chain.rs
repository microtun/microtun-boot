//! ESP-IDF second stage, SBv2 boot-chain signing and provisioning flash.
//!
//! The chip's mask ROM loads the second stage from its fixed offset
//! (`0x0` on the ESP32-C3 and ESP32-C6, `0x2000` on the ESP32-C5). The merged boot chain
//! starts at that offset and runs to the end of the factory partition.
use crate::{board::Board, flash::write_flash, image::convert, layout::EspLayout};
use anyhow::{Context, Result, ensure};
use microtun_boot_build::tools::{number, signer::Signer};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub struct Settings {
    pub profile: String,
    pub idf_bootloader: Option<PathBuf>,
    pub prebuilt_image: Option<PathBuf>,
    pub no_flash: bool,
}
impl Settings {
    pub fn from_env() -> Self {
        Self {
            profile: microtun_boot_build::tools::env("ESP_BOOTLOADER_PROFILE", "dev"),
            idf_bootloader: std::env::var_os("ESP_IDF_BOOTLOADER_BIN").map(PathBuf::from),
            prebuilt_image: std::env::var_os("MICROTUN_BOOT_ESP_IMAGE").map(PathBuf::from),
            no_flash: microtun_boot_build::tools::flag("NO_FLASH"),
        }
    }
}
pub fn build_idf(board: &Board, settings: &Settings) -> Result<Vec<u8>> {
    ensure!(
        ["dev", "release"].contains(&settings.profile.as_str()),
        "ESP_BOOTLOADER_PROFILE must be dev or release"
    );
    let board_dir = board.dir();
    let config = fs::read_to_string(board_dir.join(format!(
        "bootloader/sdkconfig.defaults.{}",
        settings.profile
    )))?;
    let offset = config
        .lines()
        .find_map(|s| s.strip_prefix("CONFIG_PARTITION_TABLE_OFFSET="))
        .context("missing IDF table offset")?;
    ensure!(
        number(offset)? == crate::layout::table_offset(board)?,
        "IDF table offset disagrees with Cargo config"
    );
    // Boards that share an IDF project differ in flash size, so the board
    // table supplies it (ESP_FLASH_SIZE below), never the sdkconfig.
    ensure!(
        !sets_flash_size(&config),
        "the IDF sdkconfig must not set the flash size; {} supplies it",
        board.name
    );
    let raw = if let Some(path) = &settings.idf_bootloader {
        fs::read(path)?
    } else {
        let dir = tempfile::tempdir()?;
        // Capture on the calling thread so HIL's libtest hides Docker's
        // BuildKit output on success and includes it for failed tests.
        microtun_boot_build::tools::progress::run_captured(
            Command::new("docker")
                .arg("build")
                .arg("--progress=plain")
                .arg("--file")
                .arg(board_dir.join("bootloader/Dockerfile.boot"))
                .arg("--build-arg")
                .arg(format!("ESP_BOOTLOADER_PROFILE={}", settings.profile))
                .arg("--build-arg")
                .arg(format!("ESP_FLASH_SIZE={}", board.idf_flash_size()?))
                .arg("--build-arg")
                .arg(format!("ESP_PARTITION_TABLE={}", board.partitions))
                .arg("--output")
                .arg(format!("type=local,dest={}", dir.path().display()))
                .arg(&board_dir),
            "Building ESP-IDF bootloader",
        )?;
        fs::read(dir.path().join("bootloader.bin"))?
    };
    // The SBv2 signature sector follows the image, rounded up to 4 KiB.
    let max_raw = board.chip.max_stage2_signed - SIGNATURE_SECTOR;
    ensure!(
        raw.first() == Some(&0xe9) && raw.len() <= max_raw as usize,
        "expected unsigned IDF ESP image <= {} KiB",
        max_raw / 1024
    );
    // Catches an ESP_IDF_BOOTLOADER_BIN built for another flash size.
    check_flash_size(board, &raw, "IDF second stage")?;
    Ok(raw)
}

/// Whether an sdkconfig selects a flash size itself.
fn sets_flash_size(config: &str) -> bool {
    config
        .lines()
        .any(|line| line.trim_start().starts_with("CONFIG_ESPTOOLPY_FLASHSIZE"))
}

/// An ESP image header records the flash size the image was built for, and
/// the second stage configures the flash chip from it. Every boot stage must
/// match the board, or slots beyond that size are out of reach.
pub fn check_flash_size(board: &Board, image: &[u8], what: &str) -> Result<()> {
    let expected = board.header_flash_size()?;
    let found = image.get(3).map(|byte| byte >> 4);
    ensure!(
        image.first() == Some(&0xe9) && found == Some(expected),
        "{what} was built for a different flash size than {} ({} MiB): \
         header flash size {found:?}, expected {expected}",
        board.name,
        board.flash_size >> 20
    );
    Ok(())
}
/// The SBv2 signature block occupies one sector after the padded image.
const SIGNATURE_SECTOR: u32 = 0x1000;
pub struct BootChain {
    pub stage2: Vec<u8>,
    pub microtun_boot: Vec<u8>,
    pub layout: EspLayout,
}
/// Sign microtun boot and the IDF second stage with `signer`'s RSA-3072 key.
pub fn boot_chain(
    board: &'static Board,
    elf: &Path,
    settings: &Settings,
    signer: &Signer,
) -> Result<BootChain> {
    crate::elf::check_bootloader(&fs::read(elf)?, board.chip)?;
    let layout = crate::layout::default_layout(board)?;
    let raw = if let Some(path) = &settings.prebuilt_image {
        ensure!(
            settings.no_flash,
            "MICROTUN_BOOT_ESP_IMAGE requires NO_FLASH=1"
        );
        fs::read(path)?
    } else {
        convert(elf, true, board)?
    };
    ensure!(
        !raw.is_empty() && raw.len().is_multiple_of(board.chip.mmu_page_size as usize),
        "microtun boot must be secure-padded to 64 KiB"
    );
    check_flash_size(board, &raw, "microtun boot")?;
    let idf = build_idf(board, settings)?;
    // The same RSA key signs the ROM's IDF stage and the microtun boot stage.
    let microtun_boot = crate::sbv2::sign(&raw, signer)?;
    let stage2 = crate::sbv2::sign(&idf, signer)?;
    crate::sbv2::verify_same_key(&microtun_boot, &stage2)?;
    let stage2_end = board.chip.rom_bootloader_offset as usize + stage2.len();
    ensure!(
        stage2.len() <= board.chip.max_stage2_signed as usize
            && stage2_end <= layout.offset as usize,
        "signed IDF exceeds loader/table limit"
    );
    ensure!(
        microtun_boot.len() <= layout.region("microtun-boot")?.1 as usize,
        "signed microtun boot exceeds factory partition"
    );
    Ok(BootChain {
        stage2,
        microtun_boot,
        layout,
    })
}
/// Output files written by [`BootChain::write`].
pub const OUTPUTS: [&str; 4] = [
    "microtun-boot.sbv2.bin",
    "bootloader.idf.sbv2.bin",
    "partitions.bin",
    MERGED,
];
/// Single image for flash from the chip's second-stage offset up to the end
/// of the factory partition.
pub const MERGED: &str = "boot-chain.sbv2.bin";
/// Find a single fully verified SBv2 stage followed only by erased flash.
/// Do not infer stage lengths from filenames or trust an SBv2 header alone.
fn signed_stage(region: &[u8], max_len: usize) -> Result<&[u8]> {
    let mut result = None;
    for end in (8192..=max_len.min(region.len())).step_by(4096) {
        if region[end - 4096..end - 4092] != [0xe7, 2, 0, 0]
            || !region[end..].iter().all(|&b| b == 0xff)
        {
            continue;
        }
        let candidate = &region[..end];
        if crate::sbv2::verify_embedded(candidate).is_ok() {
            ensure!(result.is_none(), "multiple signed stages in boot chain");
            result = Some(candidate);
        }
    }
    result.context("missing or invalid RSA-3072 signed boot stage")
}

/// Validate a pre-signed, CI-style merged boot chain before programming flash.
/// Both ROM signatures are checked using their embedded public keys, and the
/// partition table must match the compiled board layout. This does not
/// establish whether either key is already trusted by the device's eFuses.
///
/// `data` starts at the chip's second-stage offset.
pub fn verify_signed_merged(data: &[u8], layout: &EspLayout) -> Result<()> {
    let chip = layout.board.chip;
    let base = chip.rom_bootloader_offset as usize;
    let (factory, size) = layout.region("microtun-boot")?;
    ensure!(
        data.len() + base == factory as usize + size as usize,
        "signed ESP boot chain must cover {base:#x} through the factory partition"
    );
    let table = layout.binary()?;
    let table_start = layout.offset as usize - base;
    let factory_start = factory as usize - base;
    let table_end = table_start
        .checked_add(table.len())
        .context("partition table end overflow")?;
    ensure!(
        table_end <= factory_start && data.get(table_start..table_end) == Some(table.as_slice()),
        "signed ESP boot chain partition table differs from the board layout"
    );
    ensure!(
        data[table_end..factory_start].iter().all(|&b| b == 0xff),
        "unexpected data in signed boot chain between partition table and microtun boot"
    );
    let stage2 = signed_stage(&data[..table_start], chip.max_stage2_signed as usize)?;
    let microtun_boot = signed_stage(&data[factory_start..], size as usize)?;
    ensure!(
        stage2.first() == Some(&0xe9) && microtun_boot.first() == Some(&0xe9),
        "signed ESP boot stages must contain ESP images"
    );
    check_flash_size(layout.board, stage2, "signed IDF second stage")?;
    check_flash_size(layout.board, microtun_boot, "signed microtun boot")?;
    ensure!(
        (microtun_boot.len() - 4096).is_multiple_of(chip.mmu_page_size as usize),
        "signed microtun boot image must be padded to 64 KiB"
    );
    crate::sbv2::verify_same_key(stage2, microtun_boot)
}

/// Program a fully signed release/CI boot-chain binary directly at the chip's
/// second-stage offset. Neither signing keys nor a Docker build are required.
pub fn flash_signed_merged(board: &'static Board, path: &Path, no_flash: bool) -> Result<()> {
    let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    verify_signed_merged(&data, &crate::layout::default_layout(board)?)?;
    if no_flash {
        println!("NO_FLASH=1: verified existing signed boot chain; skipped flashing");
        return Ok(());
    }
    microtun_boot_build::tools::require_provisioning_ack()?;
    // Flash the exact bytes we verified even if the source file changes.
    let dir = tempfile::tempdir()?;
    let verified = dir.path().join(MERGED);
    fs::write(&verified, &data)?;
    write_flash(
        board,
        &[(board.chip.rom_bootloader_offset, &verified)],
        false,
    )
}

impl BootChain {
    pub fn write(&self, dir: &Path) -> Result<()> {
        microtun_boot_build::tools::write_output(
            &dir.join("microtun-boot.sbv2.bin"),
            &self.microtun_boot,
        )?;
        microtun_boot_build::tools::write_output(
            &dir.join("bootloader.idf.sbv2.bin"),
            &self.stage2,
        )?;
        microtun_boot_build::tools::write_output(
            &dir.join("partitions.bin"),
            self.layout.binary()?,
        )?;
        microtun_boot_build::tools::write_output(&dir.join(MERGED), self.merged()?)
    }
    /// The IDF second stage, partition table and factory microtun boot as one
    /// raw image for flash `<second-stage offset>..end of the microtun-boot
    /// partition`. Unused bytes are erased (0xFF), including any partition
    /// that lies between the table and microtun boot (nvs, phy_init).
    pub fn merged(&self) -> Result<Vec<u8>> {
        let board = self.layout.board;
        let base = board.chip.rom_bootloader_offset;
        let (factory, size) = self.layout.region("microtun-boot")?;
        let end = factory
            .checked_add(size)
            .context("factory partition overflow")?;
        let table = self.layout.binary()?;
        crate::merge::merge(
            board,
            end - base,
            &[
                (0, &self.stage2),
                (self.layout.offset - base, &table),
                (factory - base, &self.microtun_boot),
            ],
        )
    }
    pub fn flash(&self, dir: &Path) -> Result<()> {
        microtun_boot_build::tools::require_provisioning_ack()?;
        let board = self.layout.board;
        let factory = dir.join("microtun-boot.sbv2.bin");
        let table = dir.join("partitions.bin");
        let stage2 = dir.join("bootloader.idf.sbv2.bin");
        write_flash(
            board,
            &[
                (self.layout.region("microtun-boot")?.0, &factory),
                (self.layout.offset, &table),
                (board.chip.rom_bootloader_offset, &stage2),
            ],
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::{BOARDS, ESP32_C3_MINI_1, ESP32_C5_MINI_1, ESP32_C6_MINI_1};

    /// An ESP image of `len` bytes whose header names `board`'s flash size.
    fn esp_image(board: &Board, len: usize) -> Vec<u8> {
        let mut image = vec![0xe9; len];
        image[3] = board.header_flash_size().unwrap() << 4;
        image
    }

    #[test]
    fn existing_signed_ci_chain_is_verified_without_resigning_or_flashing() {
        for board in BOARDS {
            signed_ci_chain_round_trip(board);
        }
    }

    fn signed_ci_chain_round_trip(board: &'static Board) {
        let layout = crate::layout::default_layout(board).unwrap();
        let base = board.chip.rom_bootloader_offset as usize;
        let chain = BootChain {
            stage2: crate::sbv2::test_key::signed(&esp_image(board, 0x2000)),
            microtun_boot: crate::sbv2::test_key::signed(&esp_image(board, 0x10000)),
            layout,
        };
        let mut merged = chain.merged().unwrap();
        verify_signed_merged(&merged, &chain.layout).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ci-boot-chain.sbv2.bin");
        fs::write(&path, &merged).unwrap();
        flash_signed_merged(board, &path, true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), merged);

        let table = chain.layout.offset as usize - base;
        merged[table] ^= 1;
        assert!(verify_signed_merged(&merged, &chain.layout).is_err());
        merged[table] ^= 1;
        merged[0x2000] ^= 1;
        assert!(verify_signed_merged(&merged, &chain.layout).is_err());
        assert!(verify_signed_merged(&merged[..merged.len() - 1], &chain.layout).is_err());
    }

    #[test]
    fn merged_image_spans_stage2_table_and_factory_partition() {
        merged_image_layout(&ESP32_C3_MINI_1, 0x60000);
        // The C6 ROM, like the C3's, loads the second stage from 0x0.
        merged_image_layout(&ESP32_C6_MINI_1, 0x60000);
        // The C5 ROM loads the second stage from 0x2000.
        merged_image_layout(&ESP32_C5_MINI_1, 0x5e000);
    }

    /// A boot chain whose stages were built for another flash size (e.g. a
    /// reused ESP_IDF_BOOTLOADER_BIN) is refused.
    #[test]
    fn signed_chain_is_bound_to_the_board_flash_size() {
        let board = &ESP32_C5_MINI_1;
        let layout = crate::layout::default_layout(board).unwrap();
        let mut stale_stage2 = esp_image(board, 0x2000);
        stale_stage2[3] = 3 << 4; // 8 MiB
        let stale = BootChain {
            stage2: crate::sbv2::test_key::signed(&stale_stage2),
            microtun_boot: crate::sbv2::test_key::signed(&esp_image(board, 0x10000)),
            layout: crate::layout::default_layout(board).unwrap(),
        };
        let error = verify_signed_merged(&stale.merged().unwrap(), &layout).unwrap_err();
        assert!(
            error.to_string().contains("different flash size"),
            "{error}"
        );
    }

    #[test]
    fn stage_headers_must_name_the_board_flash_size() {
        for board in BOARDS {
            check_flash_size(board, &esp_image(board, 32), "stage").unwrap();
            let mut wrong = esp_image(board, 32);
            wrong[3] ^= 0x10;
            assert!(check_flash_size(board, &wrong, "stage").is_err());
            assert!(check_flash_size(board, &wrong[..3], "stage").is_err());
            let mut not_esp = esp_image(board, 32);
            not_esp[0] = 0;
            assert!(check_flash_size(board, &not_esp, "stage").is_err());
        }
        // The flash frequency in the low nibble is not checked.
        let mut image = esp_image(&ESP32_C5_MINI_1, 32);
        image[3] |= 0x0f;
        check_flash_size(&ESP32_C5_MINI_1, &image, "stage").unwrap();
    }

    #[test]
    fn flash_size_settings_are_found_but_comments_are_not() {
        assert!(sets_flash_size("A=1\nCONFIG_ESPTOOLPY_FLASHSIZE_8MB=y\n"));
        assert!(!sets_flash_size(
            "# CONFIG_ESPTOOLPY_FLASHSIZE_* comes from the board\n"
        ));
    }

    /// Both IDF projects leave the flash size to the board table.
    #[test]
    fn idf_sdkconfigs_do_not_set_the_flash_size() {
        for board in BOARDS {
            for profile in ["dev", "release"] {
                let path = board
                    .dir()
                    .join(format!("bootloader/sdkconfig.defaults.{profile}"));
                let config = fs::read_to_string(&path).unwrap();
                assert!(!sets_flash_size(&config), "{}", path.display());
            }
        }
    }

    fn merged_image_layout(board: &'static Board, expected_len: usize) {
        let layout = crate::layout::default_layout(board).unwrap();
        let base = board.chip.rom_bootloader_offset;
        let table = layout.binary().unwrap();
        let (factory, size) = layout.region("microtun-boot").unwrap();
        let chain = BootChain {
            stage2: vec![0xe9; 0x9000],
            microtun_boot: vec![0x5a; 0x30000],
            layout,
        };
        let image = chain.merged().unwrap();
        assert_eq!(image.len(), (factory + size - base) as usize);
        assert_eq!(image.len(), expected_len, "{}", board.name);
        let at = |offset: u32, len: usize| &image[(offset - base) as usize..][..len];
        assert_eq!(at(base, chain.stage2.len()), &chain.stage2[..]);
        assert_eq!(at(chain.layout.offset, table.len()), &table[..]);
        assert_eq!(
            at(factory, chain.microtun_boot.len()),
            &chain.microtun_boot[..]
        );
        // Everything else is erased flash.
        let used = [
            (0, chain.stage2.len()),
            ((chain.layout.offset - base) as usize, table.len()),
            ((factory - base) as usize, chain.microtun_boot.len()),
        ];
        for (i, &b) in image.iter().enumerate() {
            if !used.iter().any(|&(s, l)| (s..s + l).contains(&i)) {
                assert_eq!(b, crate::merge::ERASED, "byte {i:#x}");
            }
        }
    }
}
