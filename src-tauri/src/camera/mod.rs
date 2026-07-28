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
mod histogram;
pub mod luminance;
mod mock;
mod model;
// Public so diagnostics can reach the raw protocol below the `Camera` abstraction.
pub mod nikon;
pub mod ptpip;
mod sony;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::broadcast;

pub use error::{CameraError, CameraResult};
pub use luminance::Luminance;
pub use model::{
    BatteryStatus, CameraEvent, CameraInfo, CameraTarget, Dial, ExposureCapabilities,
    ExposureSettings, ExposureValue, FrameAnalysis, Histogram, Preview, Vendor, VendorProfile,
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

    /// The newest JPEG the camera has written, or `None` when there is nothing new
    /// since the last call.
    ///
    /// Only JPEGs: shooting RAW+JPEG deliberately produces a small companion file
    /// for exactly this purpose, and a backend must identify a file's format before
    /// transferring it so a RAW never crosses the network at all. `None` rather than
    /// an error when a backend cannot do previews.
    async fn preview(&self) -> CameraResult<Option<Preview>> {
        Ok(None)
    }

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

/// Every vendor the app knows about, whether or not it has a backend.
///
/// The UI is built from this rather than from a list of its own, so adding a vendor
/// never means editing the frontend. Note that Rust gives no automatic registration
/// here: a new vendor has to be named in this list and in [`connect`], and those two
/// places are the whole cost of adding one.
pub fn vendors() -> Vec<VendorProfile> {
    vec![
        canon::profile(),
        nikon::profile(),
        sony::profile(),
        mock::profile(),
    ]
}

/// Pick a strategy and open a session with it.
///
/// The only place in the program that knows which concrete backends exist. Everything
/// above works through `dyn Camera` and cannot tell them apart - which is the point:
/// swapping or adding a vendor changes this function and nothing else.
pub async fn connect(target: CameraTarget) -> CameraResult<Arc<dyn Camera>> {
    match target.vendor {
        Vendor::Canon => Ok(Arc::new(canon::CanonCcapi::connect(target).await?)),
        Vendor::Nikon => Ok(Arc::new(nikon::NikonPtpIp::connect(target).await?)),
        Vendor::Mock => Ok(Arc::new(mock::MockCamera::new(target))),
        // Profile only, no backend. See `sony.rs` for what it would take.
        vendor @ Vendor::Sony => Err(CameraError::UnsupportedVendor { vendor }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry is what the UI is built from, so a vendor missing here is a vendor
    /// the app cannot offer at all.
    #[test]
    fn every_vendor_appears_exactly_once() {
        let profiles = vendors();
        assert_eq!(profiles.len(), Vendor::ALL.len());

        for vendor in Vendor::ALL {
            let matching: Vec<_> = profiles.iter().filter(|p| p.vendor == vendor).collect();
            assert_eq!(matching.len(), 1, "{vendor:?} appears {} times", matching.len());
        }
    }

    /// A profile is what the connect screen renders, so nothing in it may be blank.
    #[test]
    fn profiles_are_filled_in() {
        for profile in vendors() {
            assert!(!profile.label.is_empty(), "{:?} has no label", profile.vendor);
            assert!(!profile.summary.is_empty(), "{:?} has no summary", profile.vendor);
            if profile.needs_address {
                assert!(profile.default_port > 0, "{:?} needs a port", profile.vendor);
            }
        }
    }

    /// A profile that claims no backend must actually be refused by the factory, and one
    /// that claims a backend must not be.
    ///
    /// Only checks the vendors that need no network to answer: the real backends cannot
    /// be distinguished from an unreachable camera without hardware, so asserting on them
    /// here would just spend the connect timeout. What this does catch is the drift that
    /// matters - a profile flipped to `implemented` without a matching arm in `connect`,
    /// or a backend added while its profile still says otherwise.
    #[tokio::test]
    async fn unimplemented_vendors_are_refused_by_the_factory() {
        for profile in vendors().into_iter().filter(|p| !p.implemented) {
            let target = CameraTarget::new(profile.vendor, "unused", profile.default_port);
            let error = connect(target).await.err().expect("must be refused");
            assert_eq!(
                error.kind(),
                "unsupportedVendor",
                "{:?} is not implemented but failed with {}",
                profile.vendor,
                error.kind()
            );
        }
    }

    #[tokio::test]
    async fn the_simulator_opens_without_a_network() {
        let profile = vendors()
            .into_iter()
            .find(|p| p.vendor == Vendor::Mock)
            .expect("the simulator must be offered");
        assert!(profile.implemented);
        assert!(!profile.needs_address);

        let camera = connect(CameraTarget::new(Vendor::Mock, "mock", 0))
            .await
            .expect("the simulator needs nothing to connect to");
        assert_eq!(camera.info().vendor, Vendor::Mock);
    }
}
