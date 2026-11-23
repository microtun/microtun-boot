//! ESP32 host tooling: ESP image checks and secure padding, Secure Boot V2
//! signing, ESP-IDF partition tables and the IDF boot chain, for every board
//! in [`board::BOARDS`].
#![forbid(unsafe_code)]

pub mod board;
pub mod chain;
pub mod elf;
pub mod flash;
pub mod image;
pub mod layout;
pub mod merge;
pub mod sbv2;

pub use board::Board;

use anyhow::Result;
use microtun_boot_build::tools::mcuboot::Geometry;

/// MCUboot geometry of the board's primary slot (`boards/<board>/layout/partitions.csv`).
pub fn geometry(board: &'static Board) -> Result<Geometry> {
    let table = layout::default_layout(board)?;
    Ok(Geometry {
        header: board.chip.header_size,
        slot: table.region("image-0")?.1,
        erase: board.chip.erase_size,
        write: 4,
        rom_fixed: None,
        class: board.chip.name,
    })
}
