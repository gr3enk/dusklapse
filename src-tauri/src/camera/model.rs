//! Data types shared by every camera backend.
//!
//! Nothing in here knows about a wire protocol. The ramping engine works on
//! these types alone, which is what keeps it independent of whether the body on
//! the other end speaks CCAPI over HTTP or PTP-IP over a raw socket.

use serde::{Deserialize, Serialize};

/// Which protocol dialect a camera speaks over the network.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Vendor {
    /// Canon CCAPI - REST/JSON over HTTP. Has to be unlocked per body through
    /// Canon's developer registration before the camera answers at all.
    Canon,
    /// Nikon PTP-IP (ISO 15740 over TCP). Not implemented yet.
    Nikon,
    /// Sony PTP-IP. Not implemented yet - Sony has no usable public Wi-Fi API,
    /// so this one needs reverse-engineered vendor opcodes.
    Sony,
    /// In-process fake camera. Lets the UI and the ramping logic be developed
    /// without a body on the desk.
    Mock,
}

impl Vendor {
    pub const ALL: [Vendor; 4] = [Vendor::Canon, Vendor::Nikon, Vendor::Sony, Vendor::Mock];

    /// Port the vendor listens on out of the box.
    pub fn default_port(self) -> u16 {
        match self {
            Vendor::Canon => 8080,
            // PTP-IP's registered port, used by both Nikon and Sony.
            Vendor::Nikon | Vendor::Sony => 15740,
            Vendor::Mock => 0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Vendor::Canon => "Canon",
            Vendor::Nikon => "Nikon",
            Vendor::Sony => "Sony",
            Vendor::Mock => "Simulator",
        }
    }
}

/// Where to reach a camera, and which dialect to use once we get there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraTarget {
    pub vendor: Vendor,
    pub host: String,
    pub port: u16,
}

impl CameraTarget {
    pub fn new(vendor: Vendor, host: impl Into<String>, port: u16) -> Self {
        Self {
            vendor,
            host: host.into(),
            port,
        }
    }
}

/// Identity a camera reports about itself once the session is up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraInfo {
    pub vendor: Vendor,
    pub manufacturer: String,
    pub model: String,
    pub serial: Option<String>,
    pub firmware: Option<String>,
    /// Protocol version actually negotiated, e.g. Canon's `ver130`.
    pub api_version: Option<String>,
}

/// The three exposure dials a ramp can move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dial {
    Shutter,
    Aperture,
    Iso,
}

impl Dial {
    pub const ALL: [Dial; 3] = [Dial::Shutter, Dial::Aperture, Dial::Iso];

    pub fn label(self) -> &'static str {
        match self {
            Dial::Shutter => "Shutter",
            Dial::Aperture => "Aperture",
            Dial::Iso => "ISO",
        }
    }
}

/// What the camera will currently let us select on each dial.
///
/// The lists are not static per body - they change with the shooting mode, the
/// attached lens and whether the camera is in live view. Re-read them after any
/// mode change instead of caching them for the whole session.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposureCapabilities {
    pub shutter: Vec<ExposureValue>,
    pub aperture: Vec<ExposureValue>,
    pub iso: Vec<ExposureValue>,
}

impl ExposureCapabilities {
    pub fn dial(&self, dial: Dial) -> &[ExposureValue] {
        match dial {
            Dial::Shutter => &self.shutter,
            Dial::Aperture => &self.aperture,
            Dial::Iso => &self.iso,
        }
    }
}

/// What the camera is set to right now.
///
/// Every field is optional because a dial can be out of our reach - aperture on
/// an adapted manual lens, shutter in a fully automatic mode.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposureSettings {
    pub shutter: Option<ExposureValue>,
    pub aperture: Option<ExposureValue>,
    pub iso: Option<ExposureValue>,
}

impl ExposureSettings {
    pub fn dial(&self, dial: Dial) -> Option<&ExposureValue> {
        match dial {
            Dial::Shutter => self.shutter.as_ref(),
            Dial::Aperture => self.aperture.as_ref(),
            Dial::Iso => self.iso.as_ref(),
        }
    }
}

/// One selectable position on a dial.
///
/// `stops` is the value's contribution to image brightness, in stops, where
/// +1.0 is one stop brighter. It is `None` for values that have no fixed
/// brightness - `bulb`, `auto` - which is exactly the set of values a ramp must
/// refuse to pick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposureValue {
    /// The token the camera itself uses on the wire. Never parse this for
    /// display; never synthesize it, only ever echo back what the camera sent.
    pub raw: String,
    /// Human-readable form for the UI.
    pub label: String,
    pub stops: Option<f32>,
}

/// Remaining charge. Cameras are vague about this, so both fields are best
/// effort: `percent` is only present when the body reports something numeric or
/// maps cleanly onto a level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryStatus {
    pub percent: Option<u8>,
    pub label: String,
}
