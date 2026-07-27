//! IPC surface.
//!
//! Thin on purpose: every command resolves the session, delegates, and returns.
//! No camera logic lives here, so the same operations stay reachable from a
//! future headless runner that has no WebView at all.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::camera::{
    BatteryStatus, CameraInfo, CameraResult, CameraTarget, Dial, ExposureCapabilities,
    ExposureSettings, Vendor,
};
use crate::session::{CameraSession, EventSink};

/// Channel the frontend listens on for anything the camera reports unprompted.
pub const CAMERA_EVENT: &str = "camera://event";

#[tauri::command]
pub async fn camera_connect(
    target: CameraTarget,
    app: AppHandle,
    session: State<'_, CameraSession>,
) -> CameraResult<CameraInfo> {
    let sink: EventSink = Arc::new(move |event| {
        // A delivery failure means the WebView is gone, which is not something the
        // camera session should die over.
        if let Err(err) = app.emit(CAMERA_EVENT, event) {
            log::warn!("could not deliver a camera event to the UI: {err}");
        }
    });
    session.connect(target, sink).await
}

#[tauri::command]
pub async fn camera_disconnect(session: State<'_, CameraSession>) -> CameraResult<()> {
    session.disconnect().await
}

/// `None` when nothing is connected. Lets the UI restore its state after a
/// reload without having to reconnect.
///
/// Wrapped in a `Result` it can never fail with: Tauri requires that of any
/// async command borrowing state.
#[tauri::command]
pub async fn camera_status(
    session: State<'_, CameraSession>,
) -> CameraResult<Option<CameraInfo>> {
    Ok(session.info().await)
}

#[tauri::command]
pub async fn camera_capabilities(
    session: State<'_, CameraSession>,
) -> CameraResult<ExposureCapabilities> {
    session.camera().await?.capabilities().await
}

#[tauri::command]
pub async fn camera_exposure(session: State<'_, CameraSession>) -> CameraResult<ExposureSettings> {
    session.camera().await?.exposure().await
}

#[tauri::command]
pub async fn camera_set_exposure(
    dial: Dial,
    value: String,
    session: State<'_, CameraSession>,
) -> CameraResult<()> {
    session.camera().await?.set_exposure(dial, &value).await
}

#[tauri::command]
pub async fn camera_shoot(autofocus: bool, session: State<'_, CameraSession>) -> CameraResult<()> {
    session.camera().await?.shoot(autofocus).await
}

#[tauri::command]
pub async fn camera_battery(
    session: State<'_, CameraSession>,
) -> CameraResult<Option<BatteryStatus>> {
    session.camera().await?.battery().await
}

/// The newest JPEG the camera has written.
///
/// Returns the raw file as an IPC binary body rather than JSON: a multi-megabyte
/// image base64-encoded inside a JSON string is a third larger again and has to be
/// parsed as text, which on a phone is the difference between a preview appearing
/// and the UI hitching.
///
/// **An empty body means "nothing new since last time"** - the same frame is never
/// sent twice. The frontend checks `byteLength`.
#[tauri::command]
pub async fn camera_preview(
    session: State<'_, CameraSession>,
) -> CameraResult<tauri::ipc::Response> {
    let preview = session.camera().await?.preview().await?;
    Ok(tauri::ipc::Response::new(
        preview.map(|preview| preview.bytes).unwrap_or_default(),
    ))
}

/// Default port per vendor, so the connect screen does not have to keep its own
/// copy of these numbers.
#[tauri::command]
pub fn camera_default_port(vendor: Vendor) -> u16 {
    vendor.default_port()
}
