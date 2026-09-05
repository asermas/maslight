//! Serial framing for directly attached controllers.
//!
//! Two formats cover almost every DIY board:
//!
//! * **Adalight** — `Ada`, then the LED count minus one as two bytes, then a
//!   checksum, then RGB data. This is what Prismatik and the stock Adalight
//!   sketch speak, so existing hardware keeps working.
//! * **TPM2** — `0xC9 0xDA`, a 16-bit payload length, the data, then `0x36`.
//!
//! The framing functions are pure so they can be tested without a port. The
//! actual port lives behind the `serial` feature.

use maslight_core::{ColorOrder, LedFrame};

/// `Ada` + count-1 + checksum + RGB payload.
pub fn adalight_frame(frame: &LedFrame, order: ColorOrder) -> Vec<u8> {
    let count = frame.len().max(1) - 1;
    let hi = (count >> 8) as u8;
    let lo = (count & 0xff) as u8;
    let mut out = Vec::with_capacity(6 + frame.len() * 3);
    out.extend_from_slice(b"Ada");
    out.push(hi);
    out.push(lo);
    out.push(hi ^ lo ^ 0x55);
    for c in &frame.rgb {
        out.extend_from_slice(&c.to_order(order));
    }
    out
}

/// TPM2 data packet.
pub fn tpm2_frame(frame: &LedFrame, order: ColorOrder) -> Vec<u8> {
    let len = frame.len() * 3;
    let mut out = Vec::with_capacity(5 + len);
    out.push(0xc9); // block start
    out.push(0xda); // data frame
    out.extend_from_slice(&(len as u16).to_be_bytes());
    for c in &frame.rgb {
        out.extend_from_slice(&c.to_order(order));
    }
    out.push(0x36); // block end
    out
}

#[cfg(feature = "serial")]
mod port {
    use std::time::Duration;

    use maslight_core::{ColorOrder, LedFrame, SerialProtocol};

    use crate::{OutputError, Sink, SinkCaps};

    /// A directly attached Adalight or TPM2 controller.
    pub struct SerialSink {
        port: Box<dyn serialport::SerialPort>,
        path: String,
        protocol: SerialProtocol,
        order: ColorOrder,
    }

    impl SerialSink {
        pub fn open(
            path: &str,
            baud: u32,
            protocol: SerialProtocol,
            order: ColorOrder,
        ) -> Result<Self, OutputError> {
            let port = serialport::new(path, baud)
                .timeout(Duration::from_millis(50))
                .open()
                .map_err(|e| OutputError::Open(format!("{path}: {e}")))?;
            Ok(Self {
                port,
                path: path.to_string(),
                protocol,
                order,
            })
        }

        /// Ports that look like a microcontroller, for the setup wizard.
        pub fn list() -> Vec<String> {
            serialport::available_ports()
                .map(|ports| ports.into_iter().map(|p| p.port_name).collect())
                .unwrap_or_default()
        }
    }

    impl Sink for SerialSink {
        fn caps(&self) -> SinkCaps {
            SinkCaps {
                max_leds: 4096,
                rgbw: false,
                max_fps: 60,
            }
        }

        fn label(&self) -> String {
            format!("Serial {}", self.path)
        }

        fn push(&mut self, frame: &LedFrame) -> Result<(), OutputError> {
            let bytes = match self.protocol {
                SerialProtocol::Adalight => super::adalight_frame(frame, self.order),
                SerialProtocol::Tpm2 => super::tpm2_frame(frame, self.order),
            };
            use std::io::Write;
            self.port
                .write_all(&bytes)
                .map_err(|e| OutputError::Send(e.to_string()))?;
            Ok(())
        }
    }
}

#[cfg(feature = "serial")]
pub use port::SerialSink;
