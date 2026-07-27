//! A camera that only exists in this process.
//!
//! Worth having for two reasons. The obvious one is developing the UI without a
//! body powered up on the desk. The less obvious one matters more: the ramping
//! engine needs to be testable against a camera whose exact value tables and
//! response timing we control, and no real camera gives you that.

use async_trait::async_trait;
use tokio::sync::Mutex;

use super::error::CameraResult;
use super::model::{
    BatteryStatus, CameraInfo, CameraTarget, Dial, ExposureCapabilities, ExposureSettings,
    ExposureValue, Vendor,
};
use super::Camera;

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

pub struct MockCamera {
    target: CameraTarget,
    info: CameraInfo,
    state: Mutex<State>,
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
            },
            target,
            state: Mutex::new(State {
                shutter: "1/125".into(),
                aperture: "f4.0".into(),
                iso: "400".into(),
                shots: 0,
            }),
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
        let mut state = self.state.lock().await;
        state.shots += 1;
        log::info!(
            "mock shot #{} at {} {} ISO {} (af: {autofocus})",
            state.shots,
            state.shutter,
            state.aperture,
            state.iso
        );
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
        assert_eq!(camera.exposure().await.unwrap().shutter.unwrap().raw, "1/60");
    }
}
