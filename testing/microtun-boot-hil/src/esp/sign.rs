//! MCUboot signing with upstream `imgtool` for the board's slots (see
//! [`crate::sign`]). Arguments follow `microtun_boot_build::tools::mcuboot`, which the
//! board's `microtun-boot-tools-esp sign` uses. The geometry comes from
//! `partitions.csv`; the class (`--cid`) is the chip name.

pub use crate::sign::{
    Key, SignOptions, corrupt_hash_tlv, corrupt_signature, flip_payload_bit, unprotected_tlvs,
};

use super::EspTarget;

/// `imgtool sign` an ESP image for the board layout's slots.
pub fn sign(
    target: &'static EspTarget,
    payload: &[u8],
    opts: &SignOptions,
) -> crate::Result<Vec<u8>> {
    crate::sign::sign(&target.board().signing, payload, opts)
}

/// Size of the MCUboot header in front of every ESP image.
pub fn header_size(target: &EspTarget) -> u32 {
    target.chip().header_size
}
