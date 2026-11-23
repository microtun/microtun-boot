//! Checked ELF conversion, ROM secure-boot sealing and deliberate Cortex-M
//! vector mutations. Whether a mutation is rejected is decided on the board
//! by `microtun_boot_rp::vector`.
use microtun_boot_build::tools::signer::Signer;
use microtun_boot_tools_rp::DEV_KEY;

#[derive(Clone, Debug)]
pub struct ArmImage(Vec<u8>);
impl ArmImage {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn stack(&self) -> u32 {
        u32::from_le_bytes(self.0[..4].try_into().unwrap())
    }
    pub fn reset(&self) -> u32 {
        u32::from_le_bytes(self.0[4..8].try_into().unwrap())
    }
    pub fn set_stack(mut self, value: u32) -> Self {
        self.0[..4].copy_from_slice(&value.to_le_bytes());
        self
    }
    pub fn set_reset(mut self, value: u32) -> Self {
        self.0[4..8].copy_from_slice(&value.to_le_bytes());
        self
    }
    pub fn truncate(mut self, len: usize) -> Self {
        self.0.truncate(len);
        self
    }
}

/// Reuse microtun-boot-tools-rp's flash LMA export, ROM load-map checks and boot-info checks.
/// In particular the bootloader ELF has SRAM VMAs; objcopy by VMA is incorrect.
pub fn from_elf(elf: &[u8], bootloader: bool) -> crate::Result<ArmImage> {
    let image =
        microtun_boot_tools_rp::elf::image(elf, bootloader).map_err(crate::Error::external)?;
    Ok(ArmImage(image.payload))
}

/// The local development ROM key is the default for RP2350 HIL, just as
/// the ESP32-C3 HIL harness uses a local ROM-signing key. A board already
/// enrolled with another key needs that key in the local store
/// (`microtun-signer local import`) and RP2350_SECURE_BOOT_KEY_ID naming it.
fn key_id_from_override(override_id: Option<String>) -> crate::Result<String> {
    match override_id {
        Some(id) if id.is_empty() => {
            Err("RP2350_SECURE_BOOT_KEY_ID is empty; unset it to use the development key".into())
        }
        Some(id) => Ok(id),
        None => Ok(DEV_KEY.key_id.into()),
    }
}

/// The local ROM signing key, checked to exist before anything is built.
pub fn rom_signer() -> crate::Result<Signer> {
    let id = key_id_from_override(std::env::var("RP2350_SECURE_BOOT_KEY_ID").ok())?;
    let signer = Signer::local(&id);
    signer.public_key().map_err(|e| {
        format!(
            "RP2350 ROM signing key {id} is not usable: {e:#}; create it with `microtun-signer local init --key-id {id} --algorithm secp256k1`, \
             or set RP2350_SECURE_BOOT_KEY_ID to the local key of a previously provisioned board"
        )
    })?;
    Ok(signer)
}

/// Convert and ROM-sign the bootloader using the board's production seal
/// pipeline. `microtun-boot-tools-rp` verifies the secp256k1 signature, embedded key,
/// Arm Secure metadata and SRAM load map before publishing the BIN.
/// No unsigned fallback is permitted for RP2350 HIL.
pub fn seal_bootloader(elf: &[u8]) -> crate::Result<Vec<u8>> {
    let signer = rom_signer()?;
    let image = microtun_boot_tools_rp::elf::image(elf, true).map_err(crate::Error::external)?;
    microtun_boot_tools_rp::seal::seal(&image, &signer, None).map_err(crate::Error::external)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_dev_rom_key_is_default() {
        assert_eq!(key_id_from_override(None).unwrap(), DEV_KEY.key_id);
    }

    #[test]
    fn explicitly_selected_rom_key_takes_precedence() {
        assert_eq!(
            key_id_from_override(Some("board-rom".into())).unwrap(),
            "board-rom"
        );
        assert!(key_id_from_override(Some(String::new())).is_err());
    }
}
