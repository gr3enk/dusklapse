//! The one camera the app is currently attached to.
//!
//! Held in Tauri's managed state so every command sees the same session.

use std::sync::Arc;

use tokio::sync::{broadcast, Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::camera::{
    self, Camera, CameraError, CameraEvent, CameraInfo, CameraResult, CameraTarget,
};

/// Where camera events go once the session picks them up.
///
/// A callback rather than a Tauri handle so this layer stays testable and knows
/// nothing about the WebView; the command layer supplies one that emits over IPC.
pub type EventSink = Arc<dyn Fn(CameraEvent) + Send + Sync>;

#[derive(Default)]
pub struct CameraSession {
    slot: RwLock<Option<Arc<dyn Camera>>>,
    /// Pumps the connected camera's events into the sink. Aborted and replaced
    /// with the camera it belongs to.
    forwarder: Mutex<Option<JoinHandle<()>>>,
}

impl CameraSession {
    pub async fn connect(
        &self,
        target: CameraTarget,
        sink: EventSink,
    ) -> CameraResult<CameraInfo> {
        // Connect before taking the lock: a probe against an unreachable host
        // blocks for the full request timeout, and the UI still needs to be able
        // to read the current session while that runs.
        let camera = camera::connect(target).await?;
        let info = camera.info().clone();

        // Subscribe before the camera becomes reachable, so nothing the body says
        // in the gap is lost.
        let forwarder = camera
            .events()
            .map(|events| tokio::spawn(forward(events, sink)));

        let previous = self.slot.write().await.replace(camera);
        if let Some(previous_forwarder) =
            std::mem::replace(&mut *self.forwarder.lock().await, forwarder)
        {
            previous_forwarder.abort();
        }
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
        if let Some(forwarder) = self.forwarder.lock().await.take() {
            forwarder.abort();
        }
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

/// Relay events until the camera goes away.
async fn forward(mut events: broadcast::Receiver<CameraEvent>, sink: EventSink) {
    loop {
        match events.recv().await {
            Ok(event) => sink(event),
            // Falling behind costs us events, not the session. Better than letting
            // a slow UI apply back pressure to the camera connection.
            Err(broadcast::error::RecvError::Lagged(missed)) => {
                log::warn!("dropped {missed} camera event(s) while busy");
            }
            Err(broadcast::error::RecvError::Closed) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::{Dial, Vendor};

    fn mock() -> CameraTarget {
        CameraTarget::new(Vendor::Mock, "mock", 0)
    }

    /// Events go nowhere in these tests; the mock has no event channel.
    fn discard() -> EventSink {
        Arc::new(|_| {})
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

        let info = session.connect(mock(), discard()).await.unwrap();
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
    ///
    /// Uses Sony because it is still unimplemented, so the failure is immediate
    /// and needs no network. Pointing this at Nikon would spend the connect
    /// timeout on an unreachable host.
    #[tokio::test]
    async fn a_failed_connect_leaves_the_session_intact() {
        let session = CameraSession::default();
        session.connect(mock(), discard()).await.unwrap();

        let error = session
            .connect(CameraTarget::new(Vendor::Sony, "10.0.0.1", 15740), discard())
            .await
            .unwrap_err();
        assert_eq!(error.kind(), "unsupportedVendor");

        assert_eq!(session.info().await.unwrap().vendor, Vendor::Mock);
    }
}
