//! Error type shared by every backend.
//!
//! This crosses the IPC boundary, so it serializes as `{ kind, message }`. The
//! frontend branches on `kind` and only ever shows `message` - matching on
//! human-readable text is how error handling rots.

use serde::ser::{Serialize, SerializeStruct, Serializer};

use super::model::Vendor;

pub type CameraResult<T> = Result<T, CameraError>;

#[derive(Debug, thiserror::Error)]
pub enum CameraError {
    #[error("no camera connected")]
    NotConnected,

    #[error("{} is not supported over Wi-Fi yet", vendor.label())]
    UnsupportedVendor { vendor: Vendor },

    /// Could not reach the camera at all: wrong IP, wrong network, camera asleep.
    #[error("could not reach the camera: {0}")]
    Transport(String),

    /// We reached it and it said no.
    #[error("camera rejected the request ({status}): {message}")]
    Rejected { status: u16, message: String },

    /// We reached it and it said "not now".
    ///
    /// Distinct from [`CameraError::Rejected`] because it means something completely different to
    /// the caller: the request was fine and will succeed shortly. On a body mid-exposure this is
    /// the normal answer, not a fault, and the only sensible response is to wait.
    #[error("the camera is busy, most likely still exposing")]
    Busy,

    /// We reached it and it said something we do not understand.
    #[error("unexpected reply from camera: {0}")]
    Protocol(String),

    /// The body does not offer this endpoint or dial.
    #[error("this camera does not expose {0}")]
    Unavailable(String),

    #[error("the value {value:?} is not selectable on the {dial} dial")]
    ValueNotSelectable { dial: &'static str, value: String },
}

impl CameraError {
    /// Stable machine-readable discriminant for the frontend.
    pub fn kind(&self) -> &'static str {
        match self {
            CameraError::NotConnected => "notConnected",
            CameraError::UnsupportedVendor { .. } => "unsupportedVendor",
            CameraError::Transport(_) => "transport",
            CameraError::Rejected { .. } => "rejected",
            CameraError::Busy => "busy",
            CameraError::Protocol(_) => "protocol",
            CameraError::Unavailable(_) => "unavailable",
            CameraError::ValueNotSelectable { .. } => "valueNotSelectable",
        }
    }
}

impl From<reqwest::Error> for CameraError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_decode() {
            CameraError::Protocol(err.to_string())
        } else {
            CameraError::Transport(err.to_string())
        }
    }
}

impl Serialize for CameraError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut out = serializer.serialize_struct("CameraError", 2)?;
        out.serialize_field("kind", self.kind())?;
        out.serialize_field("message", &self.to_string())?;
        out.end()
    }
}
