//! Read BLOCK0.SECURE_VERSION directly over the chip's built-in USB-JTAG (or
//! an external JTAG probe), without flashing or relying on firmware output.
//! Each board's [`SecureVersionEfuse`] says where the field reads back.

use super::{EspTarget, SecureVersionEfuse};

// probe-rs 0.32 moved Espressif targets and the native USB-JTAG driver into
// a separate plugin. Registration MUST precede both Lister::new() and attach,
// and only happen once in a process (multiple smoke tests share one binary).
fn register_espressif_plugin() {
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(probe_rs_espressif::register_plugin);
}

/// `EFUSE_RD_REPEAT_DATA3_REG`: BLOCK0 read word 4, holding SECURE_VERSION.
const SECURE_VERSION_OFFSET: u64 = 0x3c;
/// `EFUSE_RD_REPEAT_DATA_ERR3_REG`: read-only repeat-programming-error bits
/// for the same word.
const SECURE_VERSION_ERROR_OFFSET: u64 = 0x188;

impl SecureVersionEfuse {
    fn mask(self) -> u32 {
        (1 << self.bits) - 1
    }

    /// The floor recorded by the two registers: the number of burned bits.
    fn decode(self, chip: &str, data: u32, errors: u32) -> crate::Result<u32> {
        if (errors >> self.shift) & self.mask() != 0 {
            return Err(format!("{chip} SECURE_VERSION eFuse reports programming errors").into());
        }
        // microtun boot counts burned bits, not the numeric value of the field.
        Ok(((data >> self.shift) & self.mask()).count_ones())
    }
}

/// Read the eFuse controller's read-only, reset-latched BLOCK0 registers via
/// JTAG. The hardware refreshes these at boot; the HIL board must have booted
/// since any out-of-band programming. JTAG must be enabled on the DUT (the
/// boards' default `dev` second-stage profile leaves it enabled).
pub fn read(target: &EspTarget) -> crate::Result<u32> {
    use probe_rs::probe::{DebugProbeSelector, WireProtocol, list::Lister};
    use probe_rs::{MemoryInterface, Permissions};

    let chip = target.chip();
    let efuse = target.secure_version;
    register_espressif_plugin();
    let lister = Lister::new();
    let mut probe = match std::env::var("PROBE_RS_PROBE") {
        Ok(selector) => lister
            .open(
                selector
                    .parse::<DebugProbeSelector>()
                    .map_err(crate::Error::external)?,
            )
            .map_err(crate::Error::external)?,
        Err(std::env::VarError::NotPresent) => {
            let probes = lister.list_all();
            if probes.len() != 1 {
                if probes.is_empty() {
                    return Err(format!(
                        "no JTAG probe detected for {} after registering probe-rs-espressif; \
                         check USB connection to {}, `lsusb -d 303a:1001`, and USB \
                         permissions/udev rules; PROBE_RS_PROBE cannot select a \
                         disconnected probe",
                        chip.display, target.usb_pins
                    )
                    .into());
                }
                return Err(format!(
                    "expected exactly one {} JTAG probe (found {}); set PROBE_RS_PROBE",
                    chip.display,
                    probes.len()
                )
                .into());
            }
            probes[0].open().map_err(crate::Error::external)?
        }
        Err(error) => return Err(error.into()),
    };
    probe
        .select_protocol(WireProtocol::Jtag)
        .map_err(crate::Error::external)?;
    match std::env::var("PROBE_RS_SPEED_KHZ") {
        Ok(speed) => {
            probe
                .set_speed(speed.parse::<u32>()?)
                .map_err(crate::Error::external)?;
        }
        Err(std::env::VarError::NotPresent) => {}
        Err(error) => return Err(error.into()),
    }
    // probe-rs-espressif names its targets after the chip (`esp32c5`).
    let mut session = probe
        .attach(chip.name, Permissions::default())
        .map_err(|error| {
            crate::Error::new(format!(
                "cannot attach to {} JTAG: {error}; JTAG may be disabled by Secure Boot",
                chip.display
            ))
        })?;
    let mut core = session.core(0).map_err(crate::Error::external)?;
    let data = efuse.controller + SECURE_VERSION_OFFSET;
    let before = core.read_word_32(data).map_err(crate::Error::external)?;
    let errors = core
        .read_word_32(efuse.controller + SECURE_VERSION_ERROR_OFFSET)
        .map_err(crate::Error::external)?;
    let after = core.read_word_32(data).map_err(crate::Error::external)?;
    if before != after {
        return Err(format!(
            "{} SECURE_VERSION changed while reading over JTAG",
            chip.display
        )
        .into());
    }
    let floor = efuse.decode(chip.display, after, errors)?;
    eprintln!(
        "[security-counter] {} JTAG eFuses report current floor {floor}",
        chip.display
    );
    Ok(floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    const C3: SecureVersionEfuse = SecureVersionEfuse {
        controller: 0x6000_8800,
        shift: 14,
        bits: 16,
    };
    const C5: SecureVersionEfuse = SecureVersionEfuse {
        controller: 0x600b_4800,
        shift: 9,
        bits: 9,
    };

    #[test]
    fn counts_burned_bits_within_the_field_only() {
        // ESP32-C3: bits 14..30 of the word.
        assert_eq!(C3.decode("c3", 0, 0).unwrap(), 0);
        assert_eq!(C3.decode("c3", 0b111 << 14, 0).unwrap(), 3);
        assert_eq!(C3.decode("c3", 0xffff << 14, 0).unwrap(), 16);
        assert_eq!(C3.decode("c3", !(0xffff << 14), 0).unwrap(), 0);
        // ESP32-C5: bits 9..18; neighbouring fields (FORCE_SEND_RESUME at
        // bit 8, the PAC's reserved bits above) are not epochs.
        assert_eq!(C5.decode("c5", 0x1ff << 9, 0).unwrap(), 9);
        assert_eq!(C5.decode("c5", (1 << 8) | (0x7f << 18), 0).unwrap(), 0);
        assert_eq!(C5.decode("c5", 0b101 << 9, 0).unwrap(), 2);
    }

    #[test]
    fn programming_errors_in_the_field_are_fatal() {
        assert!(C3.decode("c3", 0, 1 << 14).is_err());
        assert!(C5.decode("c5", 0, 1 << 17).is_err());
        // Errors in other fields of the same word are not this counter's.
        assert!(C5.decode("c5", 0, 1 << 8).is_ok());
        assert!(C5.decode("c5", 0, 1 << 18).is_ok());
    }

    /// The board descriptors use exactly these fields.
    #[test]
    fn board_descriptors_match() {
        for target in crate::esp::targets() {
            let expected = match target.chip().name {
                "esp32c3" => C3,
                "esp32c5" => C5,
                other => panic!("no eFuse expectation for {other}"),
            };
            assert_eq!(target.secure_version, expected);
        }
    }
}
