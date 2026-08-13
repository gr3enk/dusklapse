//! IPC surface.
//!
//! Thin on purpose: every command resolves the session, delegates, and returns.
//! No camera logic lives here, so the same operations stay reachable from a
//! future headless runner that has no WebView at all.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::camera::patience;
use crate::camera::{
    BatteryStatus, CameraError, CameraInfo, CameraResult, CameraTarget, Dial, ExposureCapabilities,
    ExposureSettings, Vendor,
};
use crate::ramp::plan::{plan, BlockedDial};
use crate::ramp::{now_unix_seconds, RampSettings, RampState, SkyState};
use crate::session::{CameraSession, EventSink};
use crate::settings::{AppSettings, SettingsState};

/// Channel the frontend listens on for anything the camera reports unprompted.
pub const CAMERA_EVENT: &str = "camera://event";

#[tauri::command]
pub async fn camera_connect(
    target: CameraTarget,
    app: AppHandle,
    session: State<'_, CameraSession>,
) -> CameraResult<CameraInfo> {
    session.connect(target, event_sink(app)).await
}

/// Attach again to the camera the session already points at.
///
/// Takes no address: it reuses the one that was connected to, so the UI does not have to hold onto
/// it and a WebView reload cannot lose it. `NotConnected` when there was never a session to resume
/// - after an explicit disconnect there is deliberately nothing to reconnect to.
///
/// Nothing here is automatic. When a camera drops its access point, no amount of retrying reaches
/// it, and the person holding the tablet is the one who knows when the network is back.
#[tauri::command]
pub async fn camera_reconnect(
    app: AppHandle,
    session: State<'_, CameraSession>,
) -> CameraResult<CameraInfo> {
    let target = session.target().await.ok_or(CameraError::NotConnected)?;
    log::info!("reconnecting to {}:{}", target.host, target.port);
    session.connect(target, event_sink(app)).await
}

/// Where camera events go: straight out over IPC.
fn event_sink(app: AppHandle) -> EventSink {
    Arc::new(move |event| {
        // A delivery failure means the WebView is gone, which is not something the
        // camera session should die over.
        if let Err(err) = app.emit(CAMERA_EVENT, event) {
            log::warn!("could not deliver a camera event to the UI: {err}");
        }
    })
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
pub async fn camera_status(session: State<'_, CameraSession>) -> CameraResult<Option<CameraInfo>> {
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
    let camera = session.camera().await?;
    // A fixed budget rather than one read from the body. The ramp knows the shutter speed because
    // it just read it to make its decision; here that would be an extra round trip on every dial
    // someone turns, and the round trip could itself be refused by the same busy camera.
    patience::set_exposure_when_ready(
        camera.as_ref(),
        dial,
        &value,
        patience::UNKNOWN_SHUTTER_BUDGET,
    )
    .await
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

/// What the ramp is aiming for.
#[tauri::command]
pub async fn ramp_settings(ramp: State<'_, RampState>) -> CameraResult<RampSettings> {
    Ok(ramp.get().await)
}

/// Replace the ramp configuration.
///
/// Takes and returns the whole struct rather than offering a setter per field. Three
/// reasons: the UI already holds the whole thing, a half-applied configuration is never
/// something anyone wants, and returning what was stored means the frontend never has to
/// assume its write landed.
#[tauri::command]
pub async fn ramp_configure(
    settings: RampSettings,
    ramp: State<'_, RampState>,
) -> CameraResult<RampSettings> {
    Ok(ramp.set(settings).await)
}

/// Point the reference at the brightness of the frame currently on screen.
///
/// Reads the value out of the cached frame rather than accepting one from the frontend.
/// That is the point: the number the ramp holds is then provably the number that was
/// measured, with no chance of a stale or rounded value making the round trip. It also
/// means the button needs no argument.
///
/// `None` when no frame has been analysed yet - there is nothing to point at, which is a
/// normal state before the first exposure rather than an error.
#[tauri::command]
pub async fn ramp_reference_from_latest_frame(
    cache: State<'_, PreviewCache>,
    ramp: State<'_, RampState>,
) -> CameraResult<Option<RampSettings>> {
    let luminance = cache
        .0
        .lock()
        .await
        .as_ref()
        .and_then(|preview| preview.analysis.as_ref())
        .map(|analysis| analysis.luminance);

    match luminance {
        Some(luminance) => Ok(Some(
            ramp.set_reference(luminance, now_unix_seconds()).await,
        )),
        None => Ok(None),
    }
}

/// Aim the reference at the frame on screen, but only if nobody has aimed it yet.
///
/// For the first frame after connecting. `None` when the reference had already been set - by hand,
/// or by an earlier session that is still under way - which is what makes this safe to call on
/// every connect rather than only on the first one ever.
#[tauri::command]
pub async fn ramp_prime_reference(
    cache: State<'_, PreviewCache>,
    ramp: State<'_, RampState>,
) -> CameraResult<Option<RampSettings>> {
    let luminance = cache
        .0
        .lock()
        .await
        .as_ref()
        .and_then(|preview| preview.analysis.as_ref())
        .map(|analysis| analysis.luminance);

    match luminance {
        Some(luminance) => Ok(ramp.prime_reference(luminance, now_unix_seconds()).await),
        None => Ok(None),
    }
}

/// Where the sun is and what the daylight curve is doing about the reference.
///
/// `None` when the curve is switched off or has no position yet, which is also the signal the UI
/// uses to leave the readout out entirely rather than showing zeroes.
///
/// Separate from [`ramp_settings`] because this answer changes on its own: the settings only move
/// when someone moves them, the sky moves whether or not anyone is watching.
#[tauri::command]
pub async fn ramp_sky(ramp: State<'_, RampState>) -> CameraResult<Option<SkyState>> {
    Ok(ramp.get().await.sky(now_unix_seconds()))
}

/// The secondary settings.
#[tauri::command]
pub async fn settings_get(settings: State<'_, SettingsState>) -> CameraResult<AppSettings> {
    Ok(settings.get().await)
}

/// Replace the secondary settings.
///
/// Returns what was stored, so a value the backend clamped comes straight back rather than leaving
/// the UI showing a number that is not in force.
#[tauri::command]
pub async fn settings_set(
    settings: State<'_, SettingsState>,
    value: AppSettings,
) -> CameraResult<AppSettings> {
    Ok(settings.set(value).await)
}

/// Whether this build can ask the device where it is.
///
/// Only mobile carries the geolocation plugin, and a "use my location" button that is certain
/// to fail is worse than no button - the desktop UI hides it and asks for typed coordinates
/// instead. Answered here rather than sniffed in the WebView because this is a fact about how
/// the binary was compiled, which only the binary knows.
#[tauri::command]
pub fn platform_has_geolocation() -> bool {
    cfg!(mobile)
}

/// One dial move the ramp made, or tried to.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedChange {
    pub dial: Dial,
    pub from: String,
    pub to: String,
    pub gained_stops: f32,
    pub applied: bool,
}

/// What the ramp did about the frame on screen.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RampOutcome {
    pub deviation_stops: f32,
    /// The move that was made, if any. At most one per frame - the ramp steps rather than
    /// jumping, so the change is invisible in the finished sequence.
    pub change: Option<AppliedChange>,
    /// Why each dial could not be used, when none of them could.
    ///
    /// The reason the UI can say "ISO is already at its limit of 1250" instead of leaving
    /// someone to guess what ran out.
    pub blocked: Vec<BlockedDial>,
    /// Set when the camera refused the move.
    pub failed: Option<String>,
}

/// Correct the exposure for the frame on screen, if it needs it.
///
/// Reads the brightness from the frame already measured and cached, works out the correction,
/// and applies it. `None` when there is nothing to decide: no frame yet, or the ramp is
/// disarmed.
///
/// At most one dial moves per frame. See [`crate::ramp::plan`] for why stepping beats jumping.
#[tauri::command]
pub async fn ramp_apply(
    session: State<'_, CameraSession>,
    cache: State<'_, PreviewCache>,
    ramp: State<'_, RampState>,
) -> CameraResult<Option<RampOutcome>> {
    let Some(frame) = cache
        .0
        .lock()
        .await
        .as_ref()
        .and_then(|preview| preview.analysis.as_ref())
        .map(|analysis| analysis.luminance)
    else {
        return Ok(None);
    };

    let settings = ramp.get().await;
    let camera = session.camera().await?;

    // Read both before deciding: the value lists depend on the shooting mode and the lens, and
    // a plan built on a stale list would choose values the body no longer offers.
    let capabilities = camera.capabilities().await?;
    let exposure = camera.exposure().await?;

    // The target the ramp is actually holding, which the daylight curve may have walked below
    // the stored reference.
    let reference = settings.effective_reference(
        settings
            .daylight_now(now_unix_seconds())
            .map(|(_, daylight)| daylight),
    );

    let Some(correction) = plan(&settings, &capabilities, &exposure, frame, reference) else {
        return Ok(None);
    };

    let mut applied = None;
    let mut failed = None;

    if let Some(change) = &correction.change {
        // Waiting out the exposure is the difference between a correction that lands and one that
        // is refused. The ramp decides on the *analysed* frame, which is a second or three after
        // the frame itself, so at a short interval the write arrives inside the next exposure -
        // measured at an 8s interval with a 6s shutter, where only 2s of darkness exist.
        //
        // The budget comes from the shutter reading above, which is already in hand.
        let budget = patience::budget_for(&exposure);
        match patience::set_exposure_when_ready(camera.as_ref(), change.dial, &change.to, budget)
            .await
        {
            Ok(()) => {
                log::info!(
                    "ramp moved {:?} {} -> {} ({:+.2} stops)",
                    change.dial,
                    change.from,
                    change.to,
                    change.gained_stops
                );
                applied = Some(AppliedChange {
                    dial: change.dial,
                    from: change.from.clone(),
                    to: change.to.clone(),
                    gained_stops: change.gained_stops,
                    applied: true,
                });
            }
            Err(err) => {
                log::warn!(
                    "ramp could not move {:?} to {}: {err}",
                    change.dial,
                    change.to
                );
                applied = Some(AppliedChange {
                    dial: change.dial,
                    from: change.from.clone(),
                    to: change.to.clone(),
                    gained_stops: change.gained_stops,
                    applied: false,
                });
                failed = Some(err.to_string());
            }
        }
    }

    Ok(Some(RampOutcome {
        deviation_stops: correction.deviation_stops,
        change: applied,
        blocked: correction.blocked,
        failed,
    }))
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

/// Every vendor the app supports, described by the vendor modules themselves.
///
/// Replaces the per-vendor tables the frontend used to carry. Labels, hints, default
/// ports and access point addresses are facts about a camera protocol, so they belong
/// next to the code that speaks it - not duplicated in TypeScript where they drift.
#[tauri::command]
pub fn camera_vendors() -> Vec<crate::camera::VendorProfile> {
    crate::camera::vendors()
}

/// Default port per vendor, so the connect screen does not have to keep its own
/// copy of these numbers.
#[tauri::command]
pub fn camera_default_port(vendor: Vendor) -> u16 {
    vendor.default_port()
}
