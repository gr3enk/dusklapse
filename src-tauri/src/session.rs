//! The one camera the app is currently attached to.
//!
//! Held in Tauri's managed state so every command sees the same session.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::camera::{self, Camera, CameraError, CameraInfo, CameraResult, CameraTarget};

#[derive(Default)]
pub struct CameraSession {
    slot: RwLock<Option<Arc<dyn Camera>>>,
}

impl CameraSession {
    pub async fn connect(&self, target: CameraTarget) -> CameraResult<CameraInfo> {
        // Connect before taking the lock: a probe against an unreachable host
        // blocks for the full request timeout, and the UI still needs to be able
        // to read the current session while that runs.
        let camera = camera::connect(target).await?;
        let info = camera.info().clone();

        let previous = self.slot.write().await.replace(camera);
        if let Some(previous) = previous {
            // Best effort. A camera that already dropped off the network must not
            // be able to block the new connection.
            if let Err(err) = previous.disconnect().await {
                log::warn!("could not cleanly close the previous session: {err}");
            }
        }

        Ok(info)
    }

    pub async fn disconnect(&self) -> CameraResult<()> {
        match self.slot.write().await.take() {
            Some(camera) => camera.disconnect().await,
            None => Ok(()),
        }
    }

    pub async fn info(&self) -> Option<CameraInfo> {
        self.slot
            .read()
            .await
            .as_ref()
            .map(|camera| camera.info().clone())
    }

    /// Clone the handle out of the lock so callers never hold the guard across a
    /// round trip to the camera.
    pub async fn camera(&self) -> CameraResult<Arc<dyn Camera>> {
        self.slot
            .read()
            .await
            .clone()
            .ok_or(CameraError::NotConnected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Dial, Vendor};

    fn mock() -> CameraTarget {
        CameraTarget::new(Vendor::Mock, "mock", 0)
    }

    #[tokio::test]
    async fn reports_nothing_before_a_connection() {
        let session = CameraSession::default();
        assert!(session.info().await.is_none());
        // `.err()` rather than `unwrap_err()`: `Arc<dyn Camera>` is not `Debug`.
        let error = session.camera().await.err().expect("expected an error");
        assert_eq!(error.kind(), "notConnected");
    }

    #[tokio::test]
    async fn connect_use_disconnect() {
        let session = CameraSession::default();

        let info = session.connect(mock()).await.unwrap();
        assert_eq!(info.vendor, Vendor::Mock);
        assert_eq!(session.info().await.unwrap(), info);

        let camera = session.camera().await.unwrap();
        camera.set_exposure(Dial::Iso, "800").await.unwrap();
        assert_eq!(camera.exposure().await.unwrap().iso.unwrap().raw, "800");

        session.disconnect().await.unwrap();
        assert!(session.info().await.is_none());
    }

    /// A failed connect must not cost you the camera you already had - losing a
    /// running session to a typo in an IP address would be unforgivable
    /// mid-timelapse.
    #[tokio::test]
    async fn a_failed_connect_leaves_the_session_intact() {
        let session = CameraSession::default();
        session.connect(mock()).await.unwrap();

        let error = session
            .connect(CameraTarget::new(Vendor::Nikon, "10.0.0.1", 15740))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), "unsupportedVendor");

        assert_eq!(session.info().await.unwrap().vendor, Vendor::Mock);
    }
}
