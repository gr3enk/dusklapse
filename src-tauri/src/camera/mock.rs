//! A camera that only exists in this process.
//!
//! Worth having for two reasons. The obvious one is developing the UI without a
//! body powered up on the desk. The less obvious one matters more: the ramping
//! engine needs to be testable against a camera whose exact value tables and
//! response timing we control, and no real camera gives you that.

use async_trait::async_trait;
use tokio::sync::{broadcast, Mutex};

use super::error::CameraResult;
use super::model::{
    BatteryStatus, CameraEvent, CameraInfo, CameraTarget, Dial, ExposureCapabilities,
    ExposureSettings, ExposureValue, Preview, Vendor, VendorProfile,
};
use super::Camera;

/// A real JPEG, embedded so the mock exercises the real decode path.
///
/// Synthesising pixel data would let a broken decoder or a wrong pixel format pass
/// unnoticed; a genuine file is decoded by exactly the code a camera's frame goes
/// through. It is a dusk scene with deliberately dissimilar channel distributions, so
/// four separate histogram curves have something to show and a bug that collapses
/// them into one is obvious.
const MOCK_FRAME: &[u8] = include_bytes!("../../assets/mock-frame.jpg");

/// Enough for a UI that reads the latest frame; previews are never a backlog.
const EVENT_BUFFER: usize = 8;

/// Roughly a mirrorless body's third-stop shutter range, trimmed to full stops.
const SHUTTER: &[&str] = &[
    "1/8000", "1/4000", "1/2000", "1/1000", "1/500", "1/250", "1/125", "1/60", "1/30", "1/15",
    "1/8", "1/4", "1/2", "1", "2", "4", "8", "15", "30", "bulb",
];
const APERTURE: &[&str] = &[
    "f1.4", "f2.0", "f2.8", "f4.0", "f5.6", "f8.0", "f11", "f16", "f22",
];
const ISO: &[&str] = &[
    "100", "200", "400", "800", "1600", "3200", "6400", "12800", "auto",
];

/// How long a simulated exposure takes to acknowledge. Real bodies take
/// hundreds of milliseconds, and a UI that assumes instant is a UI that breaks
/// the first time it meets hardware.
const SHOT_LATENCY: std::time::Duration = std::time::Duration::from_millis(180);

pub fn profile() -> VendorProfile {
    VendorProfile {
        vendor: Vendor::Mock,
        label: Vendor::Mock.label().to_string(),
        summary: "Fake camera running in-process".into(),
        default_port: Vendor::Mock.default_port(),
        access_point_host: None,
        // Not on a network at all, so asking for an address would be theatre.
        needs_address: false,
        implemented: true,
    }
}

pub struct MockCamera {
    target: CameraTarget,
    info: CameraInfo,
    state: Mutex<State>,
    events: broadcast::Sender<CameraEvent>,
}

struct State {
    shutter: String,
    aperture: String,
    iso: String,
    shots: u32,
}

impl MockCamera {
    pub fn new(target: CameraTarget) -> Self {
        Self {
            info: CameraInfo {
                vendor: Vendor::Mock,
                manufacturer: "Dusklapse".into(),
                model: "Simulated Body".into(),
                serial: Some("MOCK-0001".into()),
                firmware: Some(env!("CARGO_PKG_VERSION").into()),
                api_version: None,
                supports_release: true,
                // Frames are announced the same way a real body announces them, so
                // the event path is exercised rather than bypassed.
                pushes_events: true,
            },
            target,
            state: Mutex::new(State {
                shutter: "1/125".into(),
                aperture: "f4.0".into(),
                iso: "400".into(),
                shots: 0,
            }),
            events: broadcast::channel(EVENT_BUFFER).0,
        }
    }
}

#[async_trait]
impl Camera for MockCamera {
    fn target(&self) -> &CameraTarget {
        &self.target
    }

    fn info(&self) -> &CameraInfo {
        &self.info
    }

    async fn capabilities(&self) -> CameraResult<ExposureCapabilities> {
        Ok(ExposureCapabilities {
            shutter: values(Dial::Shutter, SHUTTER),
            aperture: values(Dial::Aperture, APERTURE),
            iso: values(Dial::Iso, ISO),
        })
    }

    async fn exposure(&self) -> CameraResult<ExposureSettings> {
        let state = self.state.lock().await;
        Ok(ExposureSettings {
            shutter: Some(ExposureValue::from_raw(Dial::Shutter, &state.shutter)),
            aperture: Some(ExposureValue::from_raw(Dial::Aperture, &state.aperture)),
            iso: Some(ExposureValue::from_raw(Dial::Iso, &state.iso)),
        })
    }

    async fn set_exposure(&self, dial: Dial, value: &str) -> CameraResult<()> {
        let selectable = match dial {
            Dial::Shutter => SHUTTER,
            Dial::Aperture => APERTURE,
            Dial::Iso => ISO,
        };
        // A real camera refuses values outside its current ability list, so the
        // mock has to as well - otherwise the ramp looks correct here and fails
        // on hardware.
        if !selectable.contains(&value) {
            return Err(super::error::CameraError::ValueNotSelectable {
                dial: dial.label(),
                value: value.to_string(),
            });
        }

        let mut state = self.state.lock().await;
        match dial {
            Dial::Shutter => state.shutter = value.to_string(),
            Dial::Aperture => state.aperture = value.to_string(),
            Dial::Iso => state.iso = value.to_string(),
        }
        Ok(())
    }

    async fn shoot(&self, autofocus: bool) -> CameraResult<()> {
        tokio::time::sleep(SHOT_LATENCY).await;
        {
            let mut state = self.state.lock().await;
            state.shots += 1;
            log::info!(
                "mock shot #{} at {} {} ISO {} (af: {autofocus})",
                state.shots,
                state.shutter,
                state.aperture,
                state.iso
            );
        };

        // Err only means nobody is listening.
        let _ = self.events.send(CameraEvent::FrameRecorded);
        Ok(())
    }

    async fn bulb_open(&self) -> CameraResult<()> {
        log::info!("mock bulb open");
        Ok(())
    }

    async fn bulb_close(&self) -> CameraResult<()> {
        log::info!("mock bulb close");
        Ok(())
    }

    async fn battery(&self) -> CameraResult<Option<BatteryStatus>> {
        // Drains one percent per ten frames, so "battery ran out mid-sequence"
        // is reachable in a test instead of only in the field.
        let shots = self.state.lock().await.shots;
        let percent = 100u32.saturating_sub(shots / 10).min(100) as u8;
        Ok(Some(BatteryStatus {
            percent: Some(percent),
            label: format!("{percent}%"),
        }))
    }

    async fn preview(&self) -> CameraResult<Option<Preview>> {
        // Unlike a real backend this returns the same frame every time rather than
        // `None` once delivered: the point is to have something on screen whenever the
        // UI asks, not to model the camera's de-duplication.
        let analysis = super::histogram::analyse(MOCK_FRAME)?;
        Ok(Some(Preview {
            bytes: MOCK_FRAME.to_vec(),
            mime: "image/jpeg".into(),
            filename: "MOCK_0001.JPG".into(),
            pixels: (480, 320),
            analysis: Some(analysis),
        }))
    }

    fn events(&self) -> Option<broadcast::Receiver<CameraEvent>> {
        Some(self.events.subscribe())
    }

    async fn disconnect(&self) -> CameraResult<()> {
        Ok(())
    }
}

fn values(dial: Dial, raws: &[&str]) -> Vec<ExposureValue> {
    raws.iter()
        .map(|raw| ExposureValue::from_raw(dial, *raw))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::exposure::nearest;

    fn mock() -> MockCamera {
        MockCamera::new(CameraTarget::new(Vendor::Mock, "mock", 0))
    }

    #[tokio::test]
    async fn round_trips_a_dial_change() {
        let camera = mock();
        camera.set_exposure(Dial::Shutter, "1/30").await.unwrap();

        let exposure = camera.exposure().await.unwrap();
        assert_eq!(exposure.shutter.unwrap().raw, "1/30");
    }

    #[tokio::test]
    async fn refuses_a_value_the_camera_does_not_offer() {
        let camera = mock();
        let error = camera.set_exposure(Dial::Iso, "50").await.unwrap_err();
        assert_eq!(error.kind(), "valueNotSelectable");
    }

    /// The mock exists so the preview path can be exercised without hardware, which
    /// only works if its embedded frame really decodes.
    #[tokio::test]
    async fn produces_a_preview_with_four_populated_curves() {
        let camera = mock();
        let preview = camera.preview().await.unwrap().expect("mock has a frame");

        assert_eq!(preview.mime, "image/jpeg");
        assert!(preview.bytes.len() > 1000, "embedded frame looks truncated");

        let analysis = preview.analysis.expect("mock frame must decode");

        let histogram = analysis.histogram;
        assert!(histogram.pixels > 0);
        for channel in [
            &histogram.red,
            &histogram.green,
            &histogram.blue,
            &histogram.luma,
        ] {
            assert_eq!(channel.len(), 256);
            assert_eq!(channel.iter().sum::<u32>(), histogram.pixels);
        }
        // The three channels must differ, or the test frame cannot show whether the
        // four curves are actually drawn separately.
        assert_ne!(histogram.red, histogram.green);
        assert_ne!(histogram.green, histogram.blue);

        // The brightness figure has to be a usable reading, not a degenerate 0 or full
        // scale - otherwise the mock cannot stand in for a frame while the ramp is
        // developed against it.
        // Pinned against an independent re-derivation of the same definition (a
        // separate log-average of linear luminance over the very same file, computed
        // outside this codebase): 2269. The tolerance covers f32-versus-f64 rounding
        // and small IDCT differences between decoders, not a difference in the formula.
        let luminance = analysis.luminance;
        assert!(
            luminance.value.abs_diff(2269) <= 30,
            "mock frame measured {}, expected about 2269",
            luminance.value
        );
        assert!(luminance.linear > 0.0);

        // The brightness of this frame against itself is zero stops - the identity the
        // ramp's correction is built on.
        let stops = luminance.stops_from(luminance).unwrap();
        assert!(stops.abs() < 1e-6, "{stops} stops from itself");
    }

    /// The UI fetches a preview in response to this event, so a mock that never sends
    /// it cannot drive the path it exists to drive.
    #[tokio::test]
    async fn announces_each_shot_as_a_frame() {
        let camera = mock();
        let mut events = camera.events().expect("mock pushes events");

        camera.shoot(false).await.unwrap();

        assert_eq!(events.recv().await.unwrap(), CameraEvent::FrameRecorded);
    }

    /// The whole point of the stop-space design: pick a brightness, snap it onto
    /// the camera's own value list, and write that back.
    #[tokio::test]
    async fn a_one_stop_ramp_lands_on_a_selectable_value() {
        let camera = mock();
        let capabilities = camera.capabilities().await.unwrap();
        let start = camera.exposure().await.unwrap();

        let current = start.shutter.as_ref().unwrap().stops.unwrap();
        let target = nearest(&capabilities.shutter, current + 1.0).unwrap();
        assert_eq!(target.raw, "1/60");

        camera
            .set_exposure(Dial::Shutter, &target.raw)
            .await
            .unwrap();
        assert_eq!(
            camera.exposure().await.unwrap().shutter.unwrap().raw,
            "1/60"
        );
    }
}
