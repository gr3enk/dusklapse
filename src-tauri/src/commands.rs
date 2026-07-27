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

/// Everything about the newest frame except the pixels: filename, dimensions and
/// the histogram.
///
/// `None` when there is nothing new - the same frame is never fetched from the
/// camera twice.
///
/// Split from the image itself so each half travels in its natural form. The
/// metadata is small and structured, and wants to be JSON; the image is megabytes of
/// binary and must not be. Bundling them would force one of the two into the wrong
/// encoding.
#[tauri::command]
pub async fn camera_preview(
    session: State<'_, CameraSession>,
    cache: State<'_, PreviewCache>,
) -> CameraResult<Option<PreviewInfo>> {
    let Some(preview) = session.camera().await?.preview().await? else {
        return Ok(None);
    };

    let info = PreviewInfo {
        filename: preview.filename.clone(),
        width: preview.pixels.0,
        height: preview.pixels.1,
        bytes: preview.bytes.len() as u32,
        analysis: preview.analysis.clone(),
    };
    // Held for the follow-up call rather than sent now, so the caller decides when
    // to pay for the transfer across the IPC boundary.
    *cache.0.lock().await = Some(preview);
    Ok(Some(info))
}

/// The pixels of the frame [`camera_preview`] last reported.
///
/// Raw binary, not JSON: a multi-megabyte image base64-encoded inside a JSON string
/// is a third larger again and has to be parsed as text, which on a phone is the
/// difference between a preview appearing and the UI hitching.
///
/// An empty body means nothing has been fetched yet.
#[tauri::command]
pub async fn camera_preview_image(
    cache: State<'_, PreviewCache>,
) -> CameraResult<tauri::ipc::Response> {
    let bytes = cache
        .0
        .lock()
        .await
        .as_ref()
        .map(|preview| preview.bytes.clone())
        .unwrap_or_default();
    Ok(tauri::ipc::Response::new(bytes))
}

/// The most recently fetched frame, waiting to be collected by the WebView.
///
/// One frame deep on purpose. Previews are only ever looked at as "the latest one",
/// so a queue would just be memory holding images nobody will ask for.
#[derive(Default)]
pub struct PreviewCache(pub tokio::sync::Mutex<Option<crate::camera::Preview>>);

/// A frame's metadata, without its pixels.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewInfo {
    pub filename: String,
    pub width: u32,
    pub height: u32,
    /// Size of the image the follow-up call will return.
    pub bytes: u32,
    pub analysis: Option<crate::camera::FrameAnalysis>,
}

/// Default port per vendor, so the connect screen does not have to keep its own
/// copy of these numbers.
#[tauri::command]
pub fn camera_default_port(vendor: Vendor) -> u16 {
    vendor.default_port()
}
