//! UART0 serial monitor for `flash-app` and `flash-bootloader`.
//!
//! Mirrors `microtun-boot-tools-esp`'s monitor: after flashing and resetting, the
//! console's raw bytes are copied to stdout at 115200 baud until Ctrl+C.
//!
//! Unlike the ESP32-C3's native USB-Serial/JTAG port, the RP2350 console is
//! an independent USB-UART adapter that does not re-enumerate when the target
//! resets. The port is therefore opened *before* the SWD session (so a busy
//! or missing port fails before anything is flashed) and kept open across
//! the reset, so microtun boot's first boot lines are never missed.
use anyhow::{Context, Result, ensure};
use probe_rs::probe::DebugProbeSelector;
use serialport::{ClearBuffer, SerialPort, SerialPortInfo, SerialPortType};
use std::{
    io::{Read, Write},
    time::Duration,
};

/// `microtun_boot_rp::uart_logger::BAUDRATE`, used by microtun boot and the example
/// firmware (UART0 TX on GPIO0, 8N1).
pub const BAUD: u32 = 115_200;
/// Explicit console port, analogous to `ESPPORT`. Any serial device is
/// accepted, including a host's native UART such as `/dev/ttyAMA0`.
pub const PORT_ENV: &str = "RPPORT";

/// Pick the console port without guessing. An explicit `RPPORT` always wins.
/// Otherwise use the UART of the SWD probe's own USB device (the Raspberry Pi
/// Debug Probe, DAPLink, J-Link VCOM, ...), matched by VID, PID and, when the
/// probe reports one, its serial number. `Ok(None)` means no candidate.
pub(crate) fn select_port(
    explicit: Option<&str>,
    probe: &DebugProbeSelector,
    available: &[SerialPortInfo],
) -> Result<Option<String>> {
    if let Some(port) = explicit {
        return Ok(Some(port.to_owned()));
    }
    let candidates: Vec<&str> = available
        .iter()
        .filter(|info| !is_macos_tty_twin(&info.port_name, available))
        .filter_map(|info| match &info.port_type {
            SerialPortType::UsbPort(usb)
                if usb.vid == probe.vendor_id
                    && usb.pid == probe.product_id
                    && probe
                        .serial_number
                        .as_deref()
                        .is_none_or(|serial| usb.serial_number.as_deref() == Some(serial)) =>
            {
                Some(info.port_name.as_str())
            }
            _ => None,
        })
        .collect();
    ensure!(
        candidates.len() <= 1,
        "several serial ports belong to the SWD probe ({}); set {PORT_ENV} or pass --no-monitor",
        candidates.join(", ")
    );
    Ok(candidates.first().map(|port| (*port).to_owned()))
}

/// macOS lists every USB serial device twice, as `/dev/tty.*` and
/// `/dev/cu.*`. Keep only the callout (`cu`) device, as terminals do.
fn is_macos_tty_twin(name: &str, available: &[SerialPortInfo]) -> bool {
    name.strip_prefix("/dev/tty.").is_some_and(|rest| {
        let twin = format!("/dev/cu.{rest}");
        available.iter().any(|info| info.port_name == twin)
    })
}

/// An open console, held across the target reset.
pub(crate) struct Console {
    port: String,
    serial: Box<dyn SerialPort>,
}

impl Console {
    /// Resolve and open the console before the board is touched. Returns
    /// `None` (after which flashing proceeds without a monitor) only when
    /// `RPPORT` is unset and the probe exposes no serial port.
    pub(crate) fn open(probe: &DebugProbeSelector) -> Result<Option<Self>> {
        let explicit = std::env::var(PORT_ENV).ok().filter(|p| !p.is_empty());
        let ports = if explicit.is_some() {
            Vec::new()
        } else {
            serialport::available_ports().context("enumerating serial ports")?
        };
        let Some(port) = select_port(explicit.as_deref(), probe, &ports)? else {
            return Ok(None);
        };
        let serial = serialport::new(&port, BAUD)
            .timeout(Duration::from_millis(500))
            .open()
            .with_context(|| {
                format!(
                    "opening UART console {port} (is another terminal using it? \
                     set {PORT_ENV} or pass --no-monitor)"
                )
            })?;
        Ok(Some(Self { port, serial }))
    }

    /// Drop anything received before the reset (the SWD session halts the
    /// core while flashing, so this is the previous image's output).
    pub(crate) fn discard_input(&mut self) -> Result<()> {
        self.serial
            .clear(ClearBuffer::Input)
            .with_context(|| format!("clearing UART console {}", self.port))
    }

    /// Copy console bytes to stdout until Ctrl+C or a disconnect.
    pub(crate) fn run(mut self) -> Result<()> {
        let mut output = std::io::stdout().lock();
        eprintln!(
            "RP2350 serial monitor on {} at {BAUD} baud (Ctrl+C to exit)",
            self.port
        );
        let mut buf = [0u8; 1024];
        loop {
            match self.serial.read(&mut buf) {
                Ok(0) => continue,
                Ok(n) => {
                    output.write_all(&buf[..n])?;
                    output.flush()?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(e) => {
                    return Err(e).with_context(|| {
                        format!(
                            "UART console {} disconnected; reopen {PORT_ENV} to monitor",
                            self.port
                        )
                    });
                }
            }
        }
    }
}

/// Shown when monitoring was requested but no console could be found.
pub(crate) fn no_console_hint() {
    eprintln!(
        "No serial port found on the SWD probe; set {PORT_ENV} to monitor UART0 \
         (GPIO0, {BAUD} 8N1)"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serialport::UsbPortInfo;

    const DEBUG_PROBE: (u16, u16) = (0x2e8a, 0x000c);

    fn usb(name: &str, (vid, pid): (u16, u16), serial: Option<&str>) -> SerialPortInfo {
        SerialPortInfo {
            port_name: name.to_owned(),
            port_type: SerialPortType::UsbPort(UsbPortInfo {
                vid,
                pid,
                serial_number: serial.map(str::to_owned),
                manufacturer: None,
                product: None,
                interface: None,
            }),
        }
    }

    fn probe(serial: Option<&str>) -> DebugProbeSelector {
        DebugProbeSelector {
            vendor_id: DEBUG_PROBE.0,
            product_id: DEBUG_PROBE.1,
            interface: None,
            serial_number: serial.map(str::to_owned),
        }
    }

    #[test]
    fn explicit_port_wins_even_if_it_is_not_usb() {
        let ports = [usb("/dev/ttyACM0", DEBUG_PROBE, Some("A"))];
        assert_eq!(
            select_port(Some("/dev/ttyAMA0"), &probe(Some("A")), &ports).unwrap(),
            Some("/dev/ttyAMA0".to_owned())
        );
    }

    #[test]
    fn picks_the_uart_on_the_swd_probe_by_serial_number() {
        let ports = [
            usb("/dev/ttyACM0", DEBUG_PROBE, Some("A")),
            usb("/dev/ttyACM1", DEBUG_PROBE, Some("B")),
            usb("/dev/ttyUSB0", (0x0403, 0x6001), Some("B")),
        ];
        assert_eq!(
            select_port(None, &probe(Some("B")), &ports).unwrap(),
            Some("/dev/ttyACM1".to_owned())
        );
    }

    #[test]
    fn ambiguous_probe_ports_never_guess() {
        let ports = [
            usb("/dev/ttyACM0", DEBUG_PROBE, Some("A")),
            usb("/dev/ttyACM1", DEBUG_PROBE, Some("B")),
        ];
        assert!(select_port(None, &probe(None), &ports).is_err());
    }

    #[test]
    fn no_probe_uart_means_no_monitor() {
        let ports = [
            usb("/dev/ttyUSB0", (0x0403, 0x6001), None),
            SerialPortInfo {
                port_name: "/dev/ttyS0".to_owned(),
                port_type: SerialPortType::Unknown,
            },
        ];
        assert_eq!(select_port(None, &probe(Some("A")), &ports).unwrap(), None);
    }

    #[test]
    fn macos_tty_and_cu_twins_are_one_port() {
        let ports = [
            usb("/dev/tty.usbmodem1102", DEBUG_PROBE, Some("A")),
            usb("/dev/cu.usbmodem1102", DEBUG_PROBE, Some("A")),
        ];
        assert_eq!(
            select_port(None, &probe(Some("A")), &ports).unwrap(),
            Some("/dev/cu.usbmodem1102".to_owned())
        );
    }
}
