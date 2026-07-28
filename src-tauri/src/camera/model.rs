//! Data types shared by every camera backend.
//!
//! Nothing in here knows about a wire protocol. The ramping engine works on
//! these types alone, which is what keeps it independent of whether the body on
//! the other end speaks CCAPI over HTTP or PTP-IP over a raw socket.

use serde::{Deserialize, Serialize};

use super::luminance::Luminance;

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
    /// Whether this body accepts a remote shutter release.
    ///
    /// False for Nikon over Wi-Fi, which exposes no capture operation - frame
    /// timing has to come from an intervalometer there. The UI reads this rather
    /// than offering a button that is guaranteed to fail.
    pub supports_release: bool,
    /// Whether the camera pushes events rather than having to be polled.
    ///
    /// Decides how hard the UI has to poll: a body that announces its own changes
    /// needs only a slow heartbeat to notice it vanished, while one that does not
    /// has to be asked repeatedly.
    pub pushes_events: bool,
}

/// What a vendor is, before anything is connected to.
///
/// The other half of a camera backend. [`super::Camera`] describes a *live* camera;
/// this describes the vendor itself, so the UI can offer it, prefill its address and
/// grey it out when there is no backend behind it yet - all without the UI knowing
/// which vendors exist.
///
/// Each vendor module builds its own, which is what keeps facts like "Nikon answers at
/// 192.168.1.1 in access point mode" next to the code that acts on them instead of
/// scattered across the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VendorProfile {
    pub vendor: Vendor,
    pub label: String,
    /// One line, shown under the vendor picker.
    pub summary: String,
    pub default_port: u16,
    /// Address the body takes when it hosts its own network, where it has a fixed one.
    /// Prefilled so nobody has to type it.
    pub access_point_host: Option<String>,
    /// Whether an address has to be supplied at all. False for the simulator, which is
    /// not on a network.
    pub needs_address: bool,
    /// Whether a backend exists. False is not a bug - it is a vendor whose protocol has
    /// not been implemented, and the UI should say so rather than let someone try.
    pub implemented: bool,
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

/// Something the camera reported on its own initiative.
///
/// Deliberately narrow. A body volunteers far more than this - a Z 6 fires a
/// property-changed notification every time the focus point moves, eleven times in
/// five seconds while someone half-presses the shutter - and reacting to all of it
/// would be pure waste. Only what changes what the app displays or decides gets
/// through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum CameraEvent {
    /// One exposure finished.
    ///
    /// This is the frame signal, and the only trustworthy one: with an external
    /// intervalometer it is how the app learns a frame happened, so it is what a
    /// ramp advances on. Counting written files instead would double-count every
    /// frame shot as RAW+JPEG.
    FrameRecorded,
    /// A dial moved on the body itself.
    DialChanged { dial: Dial },
}

/// Tone distribution of a preview frame: the three channels separately plus
/// weighted luma, 256 bins each.
///
/// All four are kept rather than derived on the fly because they answer different
/// questions. Luma tells you whether the exposure is where you want it; the separate
/// channels tell you whether a single channel is clipping, which happens long before
/// luma looks blown - a red sunset blows red first, and a luma-only histogram hides
/// that until it is unrecoverable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Histogram {
    pub red: Vec<u32>,
    pub green: Vec<u32>,
    pub blue: Vec<u32>,
    pub luma: Vec<u32>,
    /// Pixels counted. Fewer than the full frame: the decode is scaled down, since a
    /// distribution does not need every pixel.
    pub pixels: u32,
}

impl Histogram {
    pub const BINS: usize = 256;

    pub fn empty() -> Self {
        Self {
            red: vec![0; Self::BINS],
            green: vec![0; Self::BINS],
            blue: vec![0; Self::BINS],
            luma: vec![0; Self::BINS],
            pixels: 0,
        }
    }
}

/// Everything a preview frame is measured for, from one decode.
///
/// One optional struct rather than two independent options: either the JPEG decoded and
/// both statistics exist, or it did not and neither does. Two `Option`s that are always
/// both set or both empty is an invariant waiting to be broken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameAnalysis {
    pub histogram: Histogram,
    pub luminance: Luminance,
}

/// An image pulled off the camera for on-screen review.
///
/// Not `Serialize`: the bytes cross to the WebView as a raw IPC body, because a
/// multi-megabyte JPEG re-encoded as base64 inside a JSON string is roughly a third
/// larger again and has to be parsed as text on a device with little to spare.
#[derive(Debug, Clone)]
pub struct Preview {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub filename: String,
    pub pixels: (u32, u32),
    /// `None` when the image could not be decoded. A preview that displays without
    /// measurements beats no preview at all, so a decode failure must not sink the
    /// frame it belongs to.
    pub analysis: Option<FrameAnalysis>,
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
