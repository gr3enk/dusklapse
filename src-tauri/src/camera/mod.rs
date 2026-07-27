//! Camera abstraction.
//!
//! # Layering
//!
//! `Camera` is the *semantic* layer: dials, values, shutter releases. Everything
//! above it - the intervalometer, the holy-grail ramp, the UI - talks only to
//! this trait and never learns whether the body on the other end speaks
//! REST-over-HTTP or PTP-over-TCP.
//!
//! Deliberately there is no `Transport` trait yet. Canon's CCAPI and PTP-IP have
//! so little in common at the byte level that a shared transport abstraction
//! today would be invented rather than discovered. The moment Nikon lands there
//! will be one real shared layer to extract - PTP-IP framing, which Nikon and
//! Sony both build on - and that is when it earns its place.
//!
//! # Why `&self` everywhere
//!
//! Commands run concurrently and a timelapse holds a camera for hours, so the
//! session hands out `Arc<dyn Camera>` and every method takes `&self`. Backends
//! keep their own interior mutability. This is what lets a caller clone the
//! handle out of the session lock before making a slow round trip, instead of
//! holding a lock across it.

mod canon;
mod error;
pub mod exposure;
mod mock;
mod model;
// Public so diagnostics can reach the raw protocol below the `Camera` abstraction.
pub mod nikon;
pub mod ptpip;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;

pub use error::{CameraError, CameraResult};
pub use model::{
    BatteryStatus, CameraEvent, CameraInfo, CameraTarget, Dial, ExposureCapabilities,
    ExposureSettings, ExposureValue, Vendor,
};

#[async_trait]
pub trait Camera: Send + Sync {
    fn target(&self) -> &CameraTarget;

    /// Identity, captured during connect. Cheap and infallible on purpose: the
    /// UI reads this on every render.
    fn info(&self) -> &CameraInfo;

    /// Which values each dial will currently accept.
    ///
    /// Re-read this after a mode or lens change; the lists are not fixed per
    /// body.
    async fn capabilities(&self) -> CameraResult<ExposureCapabilities>;

    /// What the camera is set to right now.
    async fn exposure(&self) -> CameraResult<ExposureSettings>;

    /// Move one dial. `value` must be a `raw` token the camera itself reported
    /// in [`Camera::capabilities`] - never a synthesized string.
    async fn set_exposure(&self, dial: Dial, value: &str) -> CameraResult<()>;

    /// Take one frame. Autofocus should stay off for a timelapse; a body that
    /// refocuses between frames produces a sequence that pops.
    async fn shoot(&self, autofocus: bool) -> CameraResult<()>;

    /// Open the shutter in bulb mode. The pair below is what pushes the ramp
    /// past the camera's longest metered speed, which every night sequence needs.
    async fn bulb_open(&self) -> CameraResult<()>;
    async fn bulb_close(&self) -> CameraResult<()>;

    /// `None` when the body does not report charge at all.
    async fn battery(&self) -> CameraResult<Option<BatteryStatus>>;

    /// Subscribe to what the camera reports unprompted, or `None` for a backend
    /// with no event channel.
    ///
    /// Where this exists it replaces polling outright: a Nikon names the property
    /// that changed, so the app can refresh exactly one dial the moment a ring is
    /// turned instead of re-reading all three on a timer and still lagging a second
    /// behind. Canon has no push channel and stays on polling.
    fn events(&self) -> Option<broadcast::Receiver<CameraEvent>> {
        None
    }

    async fn disconnect(&self) -> CameraResult<()>;
}

/// Open a session with a camera.
pub async fn connect(target: CameraTarget) -> CameraResult<Arc<dyn Camera>> {
    match target.vendor {
        Vendor::Canon => Ok(Arc::new(canon::CanonCcapi::connect(target).await?)),
        Vendor::Nikon => Ok(Arc::new(nikon::NikonPtpIp::connect(target).await?)),
        Vendor::Mock => Ok(Arc::new(mock::MockCamera::new(target))),
        // Sony has no usable public Wi-Fi API; it needs reverse-engineered vendor
        // opcodes on top of the PTP-IP layer that now exists.
        vendor @ Vendor::Sony => Err(CameraError::UnsupportedVendor { vendor }),
    }
}
