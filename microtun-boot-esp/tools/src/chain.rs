//! ESP-IDF second stage, SBv2 boot-chain signing and provisioning flash.
use crate::{flash::write_flash, image::convert, layout::EspLayout};
use anyhow::{Context, Result, ensure};
use microtun_boot_build::tools::{number, repo_root, signer::Signer};
use microtun_boot_layout_esp32c3_supermini as chip;
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
pub fn build_idf(settings: &Settings) -> Result<Vec<u8>> {
    ensure!(
        ["dev", "release"].contains(&settings.profile.as_str()),
        "ESP_BOOTLOADER_PROFILE must be dev or release"
    );
    let board = repo_root().join("boards/esp32c3-supermini");
    let config = fs::read_to_string(board.join(format!(
        "bootloader/sdkconfig.defaults.{}",
        settings.profile
    )))?;
    let offset = config
        .lines()
        .find_map(|s| s.strip_prefix("CONFIG_PARTITION_TABLE_OFFSET="))
        .context("missing IDF table offset")?;
    ensure!(
        number(offset)? == crate::layout::table_offset()?,
        "IDF table offset disagrees with Cargo config"
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
                .arg(board.join("bootloader/Dockerfile.boot"))
                .arg("--build-arg")
                .arg(format!("ESP_BOOTLOADER_PROFILE={}", settings.profile))
                .arg("--output")
                .arg(format!("type=local,dest={}", dir.path().display()))
                .arg(&board),
            "Building ESP-IDF bootloader",
        )?;
        fs::read(dir.path().join("bootloader.bin"))?
    };
    ensure!(
        raw.first() == Some(&0xe9) && raw.len() <= 0xb000,
        "expected unsigned IDF ESP image <= 44 KiB"
    );
    Ok(raw)
}
pub struct BootChain {
    pub stage2: Vec<u8>,
    pub microtun_boot: Vec<u8>,
    pub layout: EspLayout,
}
/// Sign microtun boot and the IDF second stage with `signer`'s RSA-3072 key.
pub fn boot_chain(elf: &Path, settings: &Settings, signer: &Signer) -> Result<BootChain> {
    crate::elf::check_bootloader(&fs::read(elf)?)?;
    let layout = crate::layout::default_layout()?;
    let raw = if let Some(path) = &settings.prebuilt_image {
        ensure!(
            settings.no_flash,
            "MICROTUN_BOOT_ESP_IMAGE requires NO_FLASH=1"
        );
        fs::read(path)?
    } else {
        convert(elf, true)?
    };
    ensure!(
        !raw.is_empty() && raw.len().is_multiple_of(chip::MMU_PAGE_SIZE as usize),
        "microtun boot must be secure-padded to 64 KiB"
    );
    let idf = build_idf(settings)?;
    // The same RSA key signs the ROM's IDF stage and the microtun boot stage.
    let microtun_boot = crate::sbv2::sign(&raw, signer)?;
    let stage2 = crate::sbv2::sign(&idf, signer)?;
    crate::sbv2::verify_same_key(&microtun_boot, &stage2)?;
    ensure!(
        stage2.len() <= 0xc000 && stage2.len() <= layout.offset as usize,
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
/// Single image for flash 0x0 up to the end of the factory partition.
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
pub fn verify_signed_merged(data: &[u8], layout: &EspLayout) -> Result<()> {
    let (factory, size) = layout.region("microtun-boot")?;
    ensure!(
        data.len() == factory as usize + size as usize,
        "signed ESP boot chain must cover 0x0 through the factory partition"
    );
    let table = layout.binary()?;
    let table_start = layout.offset as usize;
    let factory_start = factory as usize;
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
    let stage2 = signed_stage(&data[..table_start], 0xc000)?;
    let microtun_boot = signed_stage(&data[factory_start..], size as usize)?;
    ensure!(
        stage2.first() == Some(&0xe9) && microtun_boot.first() == Some(&0xe9),
        "signed ESP boot stages must contain ESP images"
    );
    ensure!(
        (microtun_boot.len() - 4096).is_multiple_of(chip::MMU_PAGE_SIZE as usize),
        "signed microtun boot image must be padded to 64 KiB"
    );
    crate::sbv2::verify_same_key(stage2, microtun_boot)
}

/// Program a fully signed release/CI boot-chain binary directly at flash 0x0.
/// Neither signing keys nor a Docker build are required.
pub fn flash_signed_merged(path: &Path, no_flash: bool) -> Result<()> {
    let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    verify_signed_merged(&data, &crate::layout::default_layout()?)?;
    if no_flash {
        println!("NO_FLASH=1: verified existing signed boot chain; skipped flashing");
        return Ok(());
    }
    microtun_boot_build::tools::require_provisioning_ack()?;
    // Flash the exact bytes we verified even if the source file changes.
    let dir = tempfile::tempdir()?;
    let verified = dir.path().join(MERGED);
    fs::write(&verified, &data)?;
    write_flash(&[(0, &verified)], false)
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
    /// raw image for flash `0x0..end of the microtun-boot partition`. Unused
    /// bytes are erased (0xFF), including any partition that lies between
    /// the table and microtun boot (nvs, phy_init on this board).
    pub fn merged(&self) -> Result<Vec<u8>> {
        let (factory, size) = self.layout.region("microtun-boot")?;
        let end = factory
            .checked_add(size)
            .context("factory partition overflow")?;
        let table = self.layout.binary()?;
        crate::merge::merge(
            end,
            &[
                (0, &self.stage2),
                (self.layout.offset, &table),
                (factory, &self.microtun_boot),
            ],
        )
    }
    pub fn flash(&self, dir: &Path) -> Result<()> {
        microtun_boot_build::tools::require_provisioning_ack()?;
        let factory = dir.join("microtun-boot.sbv2.bin");
        let table = dir.join("partitions.bin");
        let stage2 = dir.join("bootloader.idf.sbv2.bin");
        write_flash(
            &[
                (self.layout.region("microtun-boot")?.0, &factory),
                (self.layout.offset, &table),
                (0, &stage2),
            ],
            false,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_signed_ci_chain_is_verified_without_resigning_or_flashing() {
        let layout = crate::layout::default_layout().unwrap();
        let chain = BootChain {
            stage2: crate::sbv2::test_key::signed(&[0xe9; 0x2000]),
            microtun_boot: crate::sbv2::test_key::signed(&vec![0xe9; 0x10000]),
            layout,
        };
        let mut merged = chain.merged().unwrap();
        verify_signed_merged(&merged, &chain.layout).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ci-boot-chain.sbv2.bin");
        fs::write(&path, &merged).unwrap();
        flash_signed_merged(&path, true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), merged);

        merged[chain.layout.offset as usize] ^= 1;
        assert!(verify_signed_merged(&merged, &chain.layout).is_err());
        merged[chain.layout.offset as usize] ^= 1;
        merged[0x2000] ^= 1;
        assert!(verify_signed_merged(&merged, &chain.layout).is_err());
        assert!(verify_signed_merged(&merged[..merged.len() - 1], &chain.layout).is_err());
    }

    #[test]
    fn merged_image_spans_stage2_table_and_factory_partition() {
        let layout = crate::layout::default_layout().unwrap();
        let table = layout.binary().unwrap();
        let (factory, size) = layout.region("microtun-boot").unwrap();
        let chain = BootChain {
            stage2: vec![0xe9; 0x9000],
            microtun_boot: vec![0x5a; 0x30000],
            layout,
        };
        let image = chain.merged().unwrap();
        assert_eq!(image.len(), (factory + size) as usize);
        assert_eq!(image.len(), 0x60000, "board layout: 0x0..0x60000");
        let at = |offset: u32, len: usize| &image[offset as usize..][..len];
        assert_eq!(at(0, chain.stage2.len()), &chain.stage2[..]);
        assert_eq!(at(chain.layout.offset, table.len()), &table[..]);
        assert_eq!(
            at(factory, chain.microtun_boot.len()),
            &chain.microtun_boot[..]
        );
        // Everything else is erased flash.
        let used = [
            (0, chain.stage2.len()),
            (chain.layout.offset as usize, table.len()),
            (factory as usize, chain.microtun_boot.len()),
        ];
        for (i, &b) in image.iter().enumerate() {
            if !used.iter().any(|&(s, l)| (s..s + l).contains(&i)) {
                assert_eq!(b, crate::merge::ERASED, "byte {i:#x}");
            }
        }
    }
}
