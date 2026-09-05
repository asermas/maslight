//! # maslight-output
//!
//! Everything that turns a finished [`LedFrame`] into bytes on a wire.
//!
//! Each protocol lives in its own module as a set of **pure packet builders**,
//! so the wire format can be unit tested byte for byte without a socket. The
//! [`Sink`] implementations are thin wrappers that own a socket or a port and
//! push those packets.

use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket};

use maslight_core::{ColorOrder, DeviceConfig, LedFrame, Rgb8, WledProtocol};

pub mod artnet;
pub mod ddp;
pub mod e131;
pub mod serial;
pub mod wled;

#[cfg(feature = "discovery")]
pub mod discovery;

/// What a sink can do, so the UI can warn before the user hits a wall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SinkCaps {
    pub max_leds: usize,
    pub rgbw: bool,
    /// Sensible upper bound on frames per second for this transport.
    pub max_fps: u32,
}

impl Default for SinkCaps {
    fn default() -> Self {
        Self {
            max_leds: 65_535,
            rgbw: true,
            max_fps: 144,
        }
    }
}

/// Anything that can receive frames.
pub trait Sink: Send {
    fn caps(&self) -> SinkCaps;
    fn label(&self) -> String;
    fn push(&mut self, frame: &LedFrame) -> Result<(), OutputError>;

    /// Turn the strip off without tearing the connection down.
    fn blackout(&mut self, len: usize) -> Result<(), OutputError> {
        self.push(&LedFrame::black(len))
    }

    /// Light exactly one LED white and leave the rest dark.
    ///
    /// The calibration wizard drives this to find where each LED physically
    /// sits, so it has to work on every transport.
    fn identify(&mut self, index: usize, len: usize) -> Result<(), OutputError> {
        let mut frame = LedFrame::black(len);
        if let Some(slot) = frame.rgb.get_mut(index) {
            *slot = Rgb8::new(255, 255, 255);
        }
        self.push(&frame)
    }
}

/// Failures a transport can report.
#[derive(Debug, thiserror::Error)]
pub enum OutputError {
    #[error("could not open output: {0}")]
    Open(String),
    #[error("could not send frame: {0}")]
    Send(String),
    #[error("address {0} could not be resolved")]
    Address(String),
    #[error("this build has no support for {0}")]
    Unsupported(&'static str),
}

/// Build a sink from a profile device description.
pub fn make_sink(config: &DeviceConfig, order: ColorOrder) -> Result<Box<dyn Sink>, OutputError> {
    match config {
        DeviceConfig::Wled {
            host,
            port,
            protocol,
            timeout_s,
        } => Ok(Box::new(WledSink::new(
            host, *port, *protocol, *timeout_s, order,
        )?)),
        DeviceConfig::Ddp {
            host,
            port,
            start_offset,
        } => Ok(Box::new(DdpSink::new(host, *port, *start_offset, order)?)),
        DeviceConfig::E131 {
            host,
            universe,
            priority,
        } => Ok(Box::new(E131Sink::new(
            host.as_deref(),
            *universe,
            *priority,
            order,
        )?)),
        DeviceConfig::ArtNet {
            host,
            port,
            universe,
            net,
            subnet,
        } => Ok(Box::new(ArtNetSink::new(
            host,
            *port,
            *net,
            *subnet,
            *universe as u8,
            order,
        )?)),
        #[cfg(feature = "serial")]
        DeviceConfig::Serial {
            port,
            baud,
            protocol,
        } => Ok(Box::new(serial::SerialSink::open(
            port, *baud, *protocol, order,
        )?)),
        #[cfg(not(feature = "serial"))]
        DeviceConfig::Serial { .. } => Err(OutputError::Unsupported("serial output")),
        DeviceConfig::Null => Ok(Box::new(NullSink::default())),
    }
}

fn bind_udp(target: &SocketAddr) -> Result<UdpSocket, OutputError> {
    let bind: SocketAddr = if target.is_ipv4() {
        (Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let socket = UdpSocket::bind(bind).map_err(|e| OutputError::Open(e.to_string()))?;
    socket
        .set_nonblocking(true)
        .map_err(|e| OutputError::Open(e.to_string()))?;
    Ok(socket)
}

fn resolve(host: &str, port: u16) -> Result<SocketAddr, OutputError> {
    // A bare IP is the common case and needs no resolver at all.
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return Ok(SocketAddr::new(ip, port));
    }
    (host, port)
        .to_socket_addrs()
        .map_err(|_| OutputError::Address(host.to_string()))?
        .next()
        .ok_or_else(|| OutputError::Address(host.to_string()))
}

/// Send every datagram, reporting the first failure.
fn send_all(
    socket: &UdpSocket,
    target: &SocketAddr,
    packets: &[Vec<u8>],
) -> Result<(), OutputError> {
    for p in packets {
        match socket.send_to(p, target) {
            Ok(_) => {}
            // A full send buffer means the network is momentarily behind. The
            // next frame is milliseconds away, so dropping this one is better
            // than blocking the engine.
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(OutputError::Send(e.to_string())),
        }
    }
    Ok(())
}

/// A WLED controller driven over UDP realtime.
pub struct WledSink {
    socket: UdpSocket,
    target: SocketAddr,
    host: String,
    protocol: WledProtocol,
    timeout_s: u8,
    order: ColorOrder,
}

impl WledSink {
    pub fn new(
        host: &str,
        port: u16,
        protocol: WledProtocol,
        timeout_s: u8,
        order: ColorOrder,
    ) -> Result<Self, OutputError> {
        let target = resolve(host, port)?;
        Ok(Self {
            socket: bind_udp(&target)?,
            target,
            host: host.to_string(),
            protocol,
            timeout_s,
            order,
        })
    }
}

impl Sink for WledSink {
    fn caps(&self) -> SinkCaps {
        let max_leds = match self.protocol {
            WledProtocol::Warls => 255,
            WledProtocol::Drgb => 490,
            WledProtocol::Drgbw => 367,
            WledProtocol::Dnrgb | WledProtocol::Dnrgbw => 65_535,
        };
        SinkCaps {
            max_leds,
            rgbw: matches!(self.protocol, WledProtocol::Drgbw | WledProtocol::Dnrgbw),
            max_fps: 144,
        }
    }

    fn label(&self) -> String {
        format!("WLED {}", self.host)
    }

    fn push(&mut self, frame: &LedFrame) -> Result<(), OutputError> {
        let packets = wled::build_packets(self.protocol, self.timeout_s, frame, self.order);
        send_all(&self.socket, &self.target, &packets)
    }
}

/// A DDP receiver.
pub struct DdpSink {
    socket: UdpSocket,
    target: SocketAddr,
    host: String,
    start_offset: u32,
    order: ColorOrder,
    sequence: u8,
}

impl DdpSink {
    pub fn new(
        host: &str,
        port: u16,
        start_offset: u32,
        order: ColorOrder,
    ) -> Result<Self, OutputError> {
        let target = resolve(host, port)?;
        Ok(Self {
            socket: bind_udp(&target)?,
            target,
            host: host.to_string(),
            start_offset,
            order,
            sequence: 1,
        })
    }
}

impl Sink for DdpSink {
    fn caps(&self) -> SinkCaps {
        SinkCaps::default()
    }

    fn label(&self) -> String {
        format!("DDP {}", self.host)
    }

    fn push(&mut self, frame: &LedFrame) -> Result<(), OutputError> {
        let packets = ddp::build_packets(frame, self.order, self.start_offset, self.sequence);
        self.sequence = if self.sequence >= 15 {
            1
        } else {
            self.sequence + 1
        };
        send_all(&self.socket, &self.target, &packets)
    }
}

/// An sACN receiver, unicast or multicast.
pub struct E131Sink {
    socket: UdpSocket,
    host: Option<String>,
    first_universe: u16,
    priority: u8,
    order: ColorOrder,
    sequence: u8,
}

impl E131Sink {
    pub fn new(
        host: Option<&str>,
        universe: u16,
        priority: u8,
        order: ColorOrder,
    ) -> Result<Self, OutputError> {
        let probe: SocketAddr = (Ipv4Addr::UNSPECIFIED, e131::PORT).into();
        let socket = bind_udp(&probe)?;
        socket
            .set_multicast_ttl_v4(8)
            .map_err(|e| OutputError::Open(e.to_string()))?;
        Ok(Self {
            socket,
            host: host.map(str::to_string),
            first_universe: universe.max(1),
            priority,
            order,
            sequence: 0,
        })
    }
}

impl Sink for E131Sink {
    fn caps(&self) -> SinkCaps {
        SinkCaps {
            max_leds: e131::LEDS_PER_UNIVERSE * 64,
            rgbw: false,
            max_fps: 44,
        }
    }

    fn label(&self) -> String {
        match &self.host {
            Some(h) => format!("sACN {h} u{}", self.first_universe),
            None => format!("sACN multicast u{}", self.first_universe),
        }
    }

    fn push(&mut self, frame: &LedFrame) -> Result<(), OutputError> {
        let packets = e131::build_packets(
            frame,
            self.order,
            self.first_universe,
            self.priority,
            self.sequence,
            "MasLight",
        );
        self.sequence = self.sequence.wrapping_add(1);
        for (universe, bytes) in packets {
            let target: SocketAddr = match &self.host {
                Some(h) => resolve(h, e131::PORT)?,
                None => (e131::multicast_addr(universe), e131::PORT).into(),
            };
            send_all(&self.socket, &target, std::slice::from_ref(&bytes))?;
        }
        Ok(())
    }
}

/// An Art-Net node.
pub struct ArtNetSink {
    socket: UdpSocket,
    target: SocketAddr,
    host: String,
    net: u8,
    subnet: u8,
    first_universe: u8,
    order: ColorOrder,
    sequence: u8,
}

impl ArtNetSink {
    pub fn new(
        host: &str,
        port: u16,
        net: u8,
        subnet: u8,
        universe: u8,
        order: ColorOrder,
    ) -> Result<Self, OutputError> {
        let target = resolve(host, port)?;
        let socket = bind_udp(&target)?;
        socket
            .set_broadcast(true)
            .map_err(|e| OutputError::Open(e.to_string()))?;
        Ok(Self {
            socket,
            target,
            host: host.to_string(),
            net,
            subnet,
            first_universe: universe,
            order,
            sequence: 1,
        })
    }
}

impl Sink for ArtNetSink {
    fn caps(&self) -> SinkCaps {
        SinkCaps {
            max_leds: artnet::LEDS_PER_UNIVERSE * 16,
            rgbw: false,
            max_fps: 44,
        }
    }

    fn label(&self) -> String {
        format!("Art-Net {}", self.host)
    }

    fn push(&mut self, frame: &LedFrame) -> Result<(), OutputError> {
        let packets = artnet::build_packets(
            frame,
            self.order,
            self.net,
            self.subnet,
            self.first_universe,
            self.sequence,
        );
        self.sequence = self.sequence.wrapping_add(1);
        if self.sequence == 0 {
            self.sequence = 1;
        }
        send_all(&self.socket, &self.target, &packets)
    }
}

/// Drops every frame. Used by tests, the preview and any profile that has no
/// device attached yet.
#[derive(Debug, Default)]
pub struct NullSink {
    pub frames: u64,
    pub last: Option<LedFrame>,
}

impl Sink for NullSink {
    fn caps(&self) -> SinkCaps {
        SinkCaps::default()
    }

    fn label(&self) -> String {
        String::from("Disconnected")
    }

    fn push(&mut self, frame: &LedFrame) -> Result<(), OutputError> {
        self.frames += 1;
        self.last = Some(frame.clone());
        Ok(())
    }
}
