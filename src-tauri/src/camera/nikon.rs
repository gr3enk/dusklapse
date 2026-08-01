//! Nikon backend, on top of PTP-IP.
//!
//! # What this supports, and what it does not
//!
//! Reading and writing the three exposure dials, plus battery level. It does
//! **not** release the shutter: a Z 6 reached this way does not offer
//! `InitiateCapture`, and the vendor capture opcodes are not available in the
//! profile the body exposes over Wi-Fi. Frame timing is expected to come from an
//! external intervalometer; this backend ramps exposure between those frames.
//!
//! # Getting a body to answer: use access point mode
//!
//! Measured on a Z 6, the two Wi-Fi paths behave completely differently and only
//! one is usable.
//!
//! **Connect to smart device, camera hosting its own network - this is the one.**
//! The body answers at 192.168.1.1, stays fully operable, and shoots normally with
//! a session held open. Nothing has to be paired.
//!
//! **Connect to computer, camera joining an existing network - a dead end.** It
//! only listens while it sits on the "start the Wireless Transmitter Utility"
//! screen, and the moment it leaves that screen - including simply pressing the
//! shutter - it emits `DeviceInfoChanged` and closes the connection. That is the
//! body switching device profile, not a timeout, so there is nothing to work
//! around short of implementing Nikon's pairing handshake.
//!
//! Either way the session must answer the camera's `Probe_Request`; see
//! [`super::ptpip`]. Nothing else is needed to keep it alive.
//!
//! # Not every body reports what it is doing
//!
//! A Z 6 fills the event channel: property changes, `ObjectAdded`, `CaptureComplete`. A D5300 on
//! the same code path sends nothing at all - measured, eleven minutes of silence from session open
//! to the channel closing - while shutter, aperture and ISO read and write normally over the
//! command channel. The symptom is a camera that connects, shows its settings, accepts changes,
//! and never delivers a frame.
//!
//! Such a body does answer `GetObjectHandles`, so [`watch_card`] asks it instead. That fallback
//! turns itself on only where no event has arrived, so it costs a talkative body nothing.
//!
//! # Value encoding, measured on a Z 6 (firmware V3.80)
//!
//! | Property | Type | Encoding |
//! |---|---|---|
//! | `FNumber` 0x5007 | UINT16 | hundredths: `280` is f/2.8 |
//! | `ExposureTime` 0x500D | UINT32 | 100 µs units: `10000` is 1 s, `300000` is 30 s |
//! | `ExposureIndex` 0x500F | UINT16 | the ISO number itself |
//!
//! One caveat worth knowing before trusting an EV readout: at the fast end the
//! encoding is lossy. One unit is 1/10000 s, but the body's fastest speed is
//! 1/8000, so the shortest enumerated value is about a third of a stop off what
//! it nominally means. Irrelevant between 1/60 and 30 s, where a timelapse
//! lives; wrong if you put it on screen as an exact EV.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;

use super::error::{CameraError, CameraResult};
use super::exposure;
use super::model::{
    BatteryStatus, CameraEvent, CameraInfo, CameraTarget, Dial, ExposureCapabilities,
    ExposureSettings, ExposureValue, Preview, Vendor, VendorProfile,
};
use super::ptpip::{
    is_jpeg, EventMapper, Form, PropDesc, PtpEvent, PtpIp, EVENT_CAPTURE_COMPLETE,
    EVENT_DEVICE_PROP_CHANGED, EVENT_OBJECT_ADDED, FORMAT_EXIF_JPEG,
};
use super::Camera;

/// Shown on the camera during the handshake.
const CLIENT_NAME: &str = "Dusklapse";

const PROP_BATTERY_LEVEL: u16 = 0x5001;
const PROP_F_NUMBER: u16 = 0x5007;
const PROP_EXPOSURE_TIME: u16 = 0x500D;
const PROP_EXPOSURE_INDEX: u16 = 0x500F;

/// One `ExposureTime` unit in seconds.
const EXPOSURE_TIME_UNIT: f32 = 0.0001;
/// `FNumber` is carried in hundredths.
const F_NUMBER_SCALE: f32 = 100.0;

/// Sentinels for the two settings with no fixed duration.
const EXPOSURE_TIME_BULB: u32 = 0xFFFF_FFFF;
const EXPOSURE_TIME_TIME: u32 = 0xFFFF_FFFD;

/// How many freshly written files to remember the handles of.
///
/// One exposure in RAW+JPEG writes two, and a frame can finish while the previous
/// one is still being flushed, so a little headroom keeps the JPEG we want from
/// falling off the end before anyone asks for it.
const RECENT_HANDLES: usize = 6;

fn dial_property(dial: Dial) -> u16 {
    match dial {
        Dial::Shutter => PROP_EXPOSURE_TIME,
        Dial::Aperture => PROP_F_NUMBER,
        Dial::Iso => PROP_EXPOSURE_INDEX,
    }
}

fn property_dial(property: u16) -> Option<Dial> {
    match property {
        PROP_EXPOSURE_TIME => Some(Dial::Shutter),
        PROP_F_NUMBER => Some(Dial::Aperture),
        PROP_EXPOSURE_INDEX => Some(Dial::Iso),
        _ => None,
    }
}

/// Decide which raw PTP events are worth waking the app for.
///
/// The filter is the point. A Z 6 reports every focus-point nudge as a property
/// change - measured: eleven notifications in five seconds from FocusMode (0x500A)
/// and FocusMeteringMode (0x501C) while the shutter was half-pressed. Re-reading
/// the camera on each of those would be three PTP round trips for nothing.
///
/// `CaptureComplete` rather than `ObjectAdded` for frames, because a single
/// exposure shooting RAW+JPEG writes two files and would otherwise count twice.
/// `ObjectAdded` still matters, just not as a frame: its parameter is the handle a
/// preview is later fetched by, so it gets recorded on the way past.
fn make_mapper(recent: Arc<Mutex<VecDeque<u32>>>) -> EventMapper {
    Arc::new(move |event: &PtpEvent| match event.code {
        EVENT_CAPTURE_COMPLETE => Some(CameraEvent::FrameRecorded),

        EVENT_OBJECT_ADDED => {
            if let Some(handle) = event.params.first() {
                let mut recent = lock(&recent);
                recent.push_front(*handle);
                recent.truncate(RECENT_HANDLES);
            }
            // Recorded, not surfaced: two of these arrive per frame.
            None
        }

        EVENT_DEVICE_PROP_CHANGED => {
            let property = *event.params.first()? as u16;
            property_dial(property).map(|dial| CameraEvent::DialChanged { dial })
        }

        _ => None,
    })
}

/// How long to wait before deciding a body reports nothing.
///
/// A Z 6 fills the event channel from the moment the session opens - eleven notifications in five
/// seconds was measured while the shutter was half-pressed - so any body still silent after this
/// has to be asked rather than listened to. Long enough that an idle talkative camera is not
/// mistaken for a mute one; short enough that the first frame is not missed by much.
const SILENCE_GRACE: Duration = Duration::from_secs(15);

/// How often to ask a silent body what is on its card.
///
/// The answer is only a few hundred bytes, but the question is not cheap: listing every object on
/// the card makes the camera walk its whole filesystem, on the same card it is writing a 25 MB NEF
/// to. Measured on a D5300 at three seconds, identical 440 KiB frames took 0.7s, then 1.4s, then
/// 3.0s to fetch, and the session collapsed on the fourth - the body was being asked to search its
/// card while serving an image from it.
///
/// This is also the worst-case delay before a frame is noticed, which is why it is not slower
/// still. Five seconds is nothing against a timelapse interval.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Notice new frames on a body that never says anything.
///
/// A D5300 opens the event channel, stays silent for eleven minutes and then closes it: shutter,
/// aperture and ISO all read and write correctly over the command channel, but no `ObjectAdded`
/// or `CaptureComplete` ever arrives, so nothing downstream learns a frame exists. It does answer
/// `GetObjectHandles` - measured, 215 JPEGs listed - which makes the card itself the signal.
///
/// So: wait, and if the camera has said nothing by then, take a silent baseline of what is already
/// on the card and watch for that set to grow. Each new handle is pushed into the same ring the
/// event mapper fills and announced as a frame, so everything downstream - the preview fetch, the
/// luminance measurement, the ramp - is unaware of the difference.
///
/// This gives up permanently the moment a real event arrives. A talkative body must never have its
/// frames counted twice, and one event is proof the camera does not need this.
async fn watch_card(session: Arc<PtpIp>, recent: Arc<Mutex<VecDeque<u32>>>) {
    tokio::time::sleep(SILENCE_GRACE).await;
    if session.saw_event() {
        return;
    }

    // Filtered to JPEG, so a full card answers with the frames rather than everything on it - and
    // so one exposure in RAW+JPEG counts once, the same reason the event mapper prefers
    // `CaptureComplete` over `ObjectAdded`.
    let mut known: HashSet<u32> = match session.object_handles(FORMAT_EXIF_JPEG).await {
        Ok(handles) => handles.into_iter().collect(),
        Err(err) => {
            // Nothing further to try: this body neither reports frames nor lists them.
            log::warn!("camera reports no events and will not list its card either: {err}");
            return;
        }
    };

    log::info!(
        "camera reports no events; watching its card instead ({} frame(s) already there)",
        known.len()
    );

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;
        if session.saw_event() {
            log::info!("camera started reporting for itself; card watch stopping");
            return;
        }

        // Give way to whatever is already talking to the camera, rather than queueing behind it.
        //
        // PTP runs one transaction at a time, so a listing asked during an image transfer waits
        // anyway - it just waits in a way that makes things worse: the camera is made to search
        // its card while reading from it, and every turn missed during a slow transfer fires the
        // instant it completes, exactly when the body has a RAW to flush. Skipping costs one turn.
        if session.is_busy() {
            continue;
        }

        let listing = match session.object_handles(FORMAT_EXIF_JPEG).await {
            Ok(listing) => listing,
            Err(err) => {
                // A single failed listing is not worth giving up over - the camera may simply be
                // busy writing. A session that has really gone away ends this task by other means:
                // `disconnect` and `Drop` both abort it.
                log::debug!("could not list the card: {err}");
                continue;
            }
        };

        // Re-checked after the listing, not only before it: an event that arrived while this
        // request was in flight means the frames in the answer are the event path's to report.
        if session.saw_event() {
            log::info!("camera started reporting for itself; card watch stopping");
            return;
        }

        for handle in new_handles(&known, &listing) {
            log::info!("card grew by object {handle}");
            known.insert(handle);

            let mut recent = lock(&recent);
            recent.push_front(handle);
            recent.truncate(RECENT_HANDLES);
            drop(recent);

            session.emit(CameraEvent::FrameRecorded);
        }
    }
}

/// The handles in `listing` that are not in `known`, oldest first.
///
/// Deliberately a set difference rather than "everything after the last one seen": handles are
/// opaque, and while a Nikon happens to hand them out in ascending order, nothing in PTP promises
/// it. Removals are ignored - a deleted file is not a new frame, and the entry left behind in
/// `known` costs four bytes.
fn new_handles(known: &HashSet<u32>, listing: &[u32]) -> Vec<u32> {
    listing
        .iter()
        .copied()
        .filter(|handle| !known.contains(handle))
        .collect()
}

/// A poisoned lock here means a thread panicked while pushing a file handle. The
/// contents are still a valid list of handles, and giving up on previews for the
/// rest of the session would be the worse outcome.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub fn profile() -> VendorProfile {
    VendorProfile {
        vendor: Vendor::Nikon,
        label: Vendor::Nikon.label().to_string(),
        summary: "PTP-IP - use 'connect to smart device' and join the camera's own Wi-Fi".into(),
        default_port: Vendor::Nikon.default_port(),
        // Measured on a Z 6 in access point mode. See the module documentation for why
        // this is the only path that works.
        access_point_host: Some("192.168.1.1".into()),
        needs_address: true,
        implemented: true,
        developer_only: false,
    }
}

pub struct NikonPtpIp {
    target: CameraTarget,
    /// Shared, because the card watch below runs on its own and needs the same session.
    session: Arc<PtpIp>,
    info: CameraInfo,
    /// Handles of recently written files, newest first, filled in by the event
    /// mapper. Both RAW and JPEG land here; which is which is only known after
    /// asking the camera.
    recent: Arc<Mutex<VecDeque<u32>>>,
    /// The last handle handed to the UI, so a second request for the same frame
    /// does not pull the same megabytes across again.
    delivered: Mutex<Option<u32>>,
    /// The card watch, so it stops when the camera goes away rather than polling a
    /// dead session forever.
    watch: tokio::task::JoinHandle<()>,
}

impl Drop for NikonPtpIp {
    fn drop(&mut self) {
        self.watch.abort();
    }
}

impl NikonPtpIp {
    pub async fn connect(target: CameraTarget) -> CameraResult<Self> {
        let recent = Arc::new(Mutex::new(VecDeque::with_capacity(RECENT_HANDLES)));
        let session = Arc::new(
            PtpIp::connect(
                &target.host,
                target.port,
                CLIENT_NAME,
                make_mapper(recent.clone()),
            )
            .await?,
        );

        let watch = tokio::spawn(watch_card(session.clone(), recent.clone()));

        let device = session.device_info();
        let info = CameraInfo {
            vendor: Vendor::Nikon,
            manufacturer: non_empty(&device.manufacturer).unwrap_or_else(|| "Nikon".to_string()),
            model: non_empty(&device.model).unwrap_or_else(|| "Unknown body".to_string()),
            // Nikon pads the serial to 32 characters with leading zeros.
            serial: non_empty(device.serial.trim_start_matches('0')),
            firmware: non_empty(&device.device_version),
            api_version: None,
            // No capture operation is reachable over Wi-Fi on this body.
            supports_release: false,
            // PTP-IP's event channel names the property that changed.
            pushes_events: true,
        };

        Ok(Self {
            target,
            session,
            info,
            recent,
            delivered: Mutex::new(None),
            watch,
        })
    }

    async fn read_dial(&self, dial: Dial) -> CameraResult<PropDesc> {
        self.session.prop_desc(dial_property(dial)).await
    }

    /// Every event the camera sends, unfiltered - for diagnostics. The filtered
    /// stream the app runs on is [`Camera::events`].
    pub fn raw_events(&self) -> tokio::sync::broadcast::Receiver<PtpEvent> {
        self.session.subscribe()
    }
}

#[async_trait]
impl Camera for NikonPtpIp {
    fn target(&self) -> &CameraTarget {
        &self.target
    }

    fn info(&self) -> &CameraInfo {
        &self.info
    }

    async fn capabilities(&self) -> CameraResult<ExposureCapabilities> {
        // Sequential, unlike the Canon backend: a PTP session is one transaction
        // at a time, so there is nothing to gain from issuing these in parallel.
        Ok(ExposureCapabilities {
            shutter: selectable(Dial::Shutter, &self.read_dial(Dial::Shutter).await?),
            aperture: selectable(Dial::Aperture, &self.read_dial(Dial::Aperture).await?),
            iso: selectable(Dial::Iso, &self.read_dial(Dial::Iso).await?),
        })
    }

    async fn exposure(&self) -> CameraResult<ExposureSettings> {
        Ok(ExposureSettings {
            shutter: Some(exposure_value(
                Dial::Shutter,
                self.read_dial(Dial::Shutter).await?.current.as_u32(),
            )),
            aperture: Some(exposure_value(
                Dial::Aperture,
                self.read_dial(Dial::Aperture).await?.current.as_u32(),
            )),
            iso: Some(exposure_value(
                Dial::Iso,
                self.read_dial(Dial::Iso).await?.current.as_u32(),
            )),
        })
    }

    async fn set_exposure(&self, dial: Dial, value: &str) -> CameraResult<()> {
        let raw: u32 = value.parse().map_err(|_| CameraError::ValueNotSelectable {
            dial: dial.label(),
            value: value.to_string(),
        })?;

        // Read the descriptor first. It costs one round trip and buys two things:
        // the width to encode the write at, and the camera's own answer to
        // whether this value is currently selectable.
        let desc = self.read_dial(dial).await?;

        if !desc.writable {
            return Err(CameraError::Unavailable(format!(
                "{} - the camera reports it as read-only right now",
                dial.label()
            )));
        }
        if let Form::Enumeration(values) = &desc.form {
            if !values.iter().any(|candidate| candidate.as_u32() == raw) {
                return Err(CameraError::ValueNotSelectable {
                    dial: dial.label(),
                    value: value.to_string(),
                });
            }
        }

        self.session
            .set_prop(dial_property(dial), desc.datatype, raw)
            .await
    }

    async fn shoot(&self, _autofocus: bool) -> CameraResult<()> {
        Err(CameraError::Unavailable(
            "the shutter - this body does not accept a remote release over Wi-Fi, \
             so frame timing has to come from an intervalometer"
                .into(),
        ))
    }

    async fn bulb_open(&self) -> CameraResult<()> {
        self.shoot(false).await
    }

    async fn bulb_close(&self) -> CameraResult<()> {
        self.shoot(false).await
    }

    async fn battery(&self) -> CameraResult<Option<BatteryStatus>> {
        match self.session.prop_desc(PROP_BATTERY_LEVEL).await {
            Ok(desc) => {
                let percent = desc.current.as_u32().min(100) as u8;
                Ok(Some(BatteryStatus {
                    percent: Some(percent),
                    label: format!("{percent}%"),
                }))
            }
            // A body that does not report charge is not a broken session.
            Err(CameraError::Unavailable(_)) => Ok(None),
            Err(err) => Err(err),
        }
    }

    async fn preview(&self) -> CameraResult<Option<Preview>> {
        // Newest first, so the first JPEG found is the current frame's.
        let candidates: Vec<u32> = lock(&self.recent).iter().copied().collect();

        for handle in candidates {
            // Ask what it is before moving a byte of it. This is what keeps a NEF
            // off the network: the answer costs a few dozen bytes.
            let info = match self.session.object_info(handle).await {
                Ok(info) => info,
                Err(err) => {
                    // A handle can go stale - deleted, or the card swapped. Not a
                    // reason to give up on the ones behind it.
                    // Info, not debug: on a body whose preview never arrives this is one of the
                    // two lines that says why, and the log view a user can reach only shows info.
                    log::info!("could not read object {handle}: {err}");
                    continue;
                }
            };

            if !is_jpeg(info.format) {
                // Info for the same reason: a body that reports its JPEG under a format code
                // `is_jpeg` does not know looks exactly like a body that sends nothing at all.
                log::info!(
                    "skipping {} - format 0x{:04x} is not a JPEG",
                    info.filename,
                    info.format
                );
                continue;
            }

            if *lock(&self.delivered) == Some(handle) {
                // Newest JPEG is the one already on screen.
                return Ok(None);
            }

            log::info!(
                "fetching {} ({} KiB, {}x{})",
                info.filename,
                info.compressed_size / 1024,
                info.pixel_width,
                info.pixel_height
            );
            let started = std::time::Instant::now();
            let bytes = self.session.object(handle).await?;
            log::info!(
                "fetched {} - {} KiB in {:.1}s",
                info.filename,
                bytes.len() / 1024,
                started.elapsed().as_secs_f32()
            );

            // Decoded here rather than in the WebView so the curves on screen are the
            // same data auto-ramping will read. A failure is logged and dropped: the
            // image itself is still worth showing.
            let analysis = match super::histogram::analyse(&bytes) {
                Ok(analysis) => {
                    log::info!(
                        "{} measures {} on the brightness scale",
                        info.filename,
                        analysis.luminance.value
                    );
                    Some(analysis)
                }
                Err(err) => {
                    log::warn!("could not measure {}: {err}", info.filename);
                    None
                }
            };

            *lock(&self.delivered) = Some(handle);
            return Ok(Some(Preview {
                bytes,
                mime: "image/jpeg".into(),
                filename: info.filename,
                pixels: (info.pixel_width, info.pixel_height),
                analysis,
            }));
        }

        Ok(None)
    }

    fn events(&self) -> Option<tokio::sync::broadcast::Receiver<CameraEvent>> {
        Some(self.session.subscribe_camera())
    }

    async fn disconnect(&self) -> CameraResult<()> {
        // Before the session closes, so the watch cannot spend its next poll on a dead socket.
        self.watch.abort();
        self.session.close().await
    }
}

/// Turn a descriptor's enumeration into the dial's selectable values.
///
/// A property constrained by a range rather than an enumeration yields nothing:
/// we would have to invent the step positions, and a ramp snapping onto invented
/// values would be rejected by the camera on write.
fn selectable(dial: Dial, desc: &PropDesc) -> Vec<ExposureValue> {
    match &desc.form {
        Form::Enumeration(values) => values
            .iter()
            .map(|value| exposure_value(dial, value.as_u32()))
            .collect(),
        _ => Vec::new(),
    }
}

fn exposure_value(dial: Dial, raw: u32) -> ExposureValue {
    let token = raw.to_string();

    match dial {
        Dial::Shutter => match raw {
            EXPOSURE_TIME_BULB => sentinel(token, "BULB"),
            EXPOSURE_TIME_TIME => sentinel(token, "TIME"),
            0 => sentinel(token, "-"),
            units => {
                let encoded = units as f32 * EXPOSURE_TIME_UNIT;
                // Undo the transport's rounding. The camera really does expose
                // 1/1600, it just cannot say so in 100 µs units, and a ramp is
                // better off with the true value than with 1/1667.
                let (seconds, label) = match exposure::snap_shutter(encoded) {
                    Some((nominal, label)) => (nominal, label.to_string()),
                    None => (encoded, exposure::shutter_label(encoded)),
                };
                ExposureValue {
                    raw: token,
                    label,
                    stops: Some(exposure::shutter_stops(seconds)),
                }
            }
        },
        Dial::Aperture => {
            if raw == 0 {
                return sentinel(token, "-");
            }
            let f_number = raw as f32 / F_NUMBER_SCALE;
            ExposureValue {
                raw: token,
                label: exposure::aperture_label(f_number),
                stops: Some(exposure::aperture_stops(f_number)),
            }
        }
        Dial::Iso => {
            if raw == 0 {
                return sentinel(token, "AUTO");
            }
            let iso = raw as f32;
            ExposureValue {
                raw: token,
                label: exposure::iso_label(iso),
                stops: Some(exposure::iso_stops(iso)),
            }
        }
    }
}

/// A value with no fixed brightness. `stops: None` is what keeps a ramp from
/// selecting it.
fn sentinel(raw: String, label: &str) -> ExposureValue {
    ExposureValue {
        raw,
        label: label.to_string(),
        stops: None,
    }
}

fn non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-3, "{a} != {b}");
    }

    /// The point of the baseline: a card with 215 pictures on it has not just taken 215 frames.
    #[test]
    fn a_full_card_is_not_a_burst_of_frames() {
        let listing: Vec<u32> = (1..=215).collect();
        let known: HashSet<u32> = listing.iter().copied().collect();

        assert!(new_handles(&known, &listing).is_empty());
    }

    #[test]
    fn a_frame_written_since_the_baseline_is_new() {
        let known: HashSet<u32> = [10, 11, 12].into_iter().collect();

        assert_eq!(new_handles(&known, &[10, 11, 12, 13]), vec![13]);
    }

    /// Several at once happens after a gap - a failed listing, or frames closer together than the
    /// poll interval - and each is still its own frame.
    #[test]
    fn several_new_frames_come_back_oldest_first() {
        let known: HashSet<u32> = [10].into_iter().collect();

        assert_eq!(new_handles(&known, &[10, 11, 12, 13]), vec![11, 12, 13]);
    }

    /// Deleting a picture on the camera shortens the listing. Nothing about that is a new frame,
    /// and the next real one must still be found.
    #[test]
    fn deletions_are_not_frames() {
        let known: HashSet<u32> = [10, 11, 12].into_iter().collect();

        assert!(new_handles(&known, &[10, 12]).is_empty());
        assert_eq!(new_handles(&known, &[10, 12, 13]), vec![13]);
    }

    /// Handles are opaque. A body that reuses a low number after a card format must not have it
    /// dismissed just because a larger one was seen earlier.
    #[test]
    fn a_handle_below_the_highest_seen_still_counts() {
        let known: HashSet<u32> = [50, 60].into_iter().collect();

        assert_eq!(new_handles(&known, &[50, 60, 7]), vec![7]);
    }

    #[test]
    fn decodes_exposure_time_units() {
        // 10000 units is one second, the anchor of the stop scale.
        let one_second = exposure_value(Dial::Shutter, 10_000);
        assert_eq!(one_second.label, "1s");
        approx(one_second.stops.unwrap(), 0.0);

        // The Z 6's longest metered speed.
        let thirty = exposure_value(Dial::Shutter, 300_000);
        assert_eq!(thirty.label, "30s");
        approx(thirty.stops.unwrap(), 30f32.log2());

        // The value the camera reported while probing.
        assert_eq!(exposure_value(Dial::Shutter, 25).label, "1/400");
    }

    /// The encoding cannot express the fast speeds exactly, so both the label and
    /// the stop value have to be recovered from the standard series - otherwise
    /// the app shows 1/1667 next to a camera displaying 1/1600.
    #[test]
    fn recovers_the_speeds_the_encoding_mangles() {
        // 6 units is 1/1667 literally, 1/1600 in intent.
        let fast = exposure_value(Dial::Shutter, 6);
        assert_eq!(fast.label, "1/1600");
        approx(fast.stops.unwrap(), (1.0f32 / 1600.0).log2());

        // The worst case: one unit is 1/10000, but the body's fastest is 1/8000.
        assert_eq!(exposure_value(Dial::Shutter, 1).label, "1/8000");
        // And the three that read as 1/323, 1/161 and 1/1250 before snapping.
        assert_eq!(exposure_value(Dial::Shutter, 31).label, "1/320");
        assert_eq!(exposure_value(Dial::Shutter, 62).label, "1/160");
        assert_eq!(exposure_value(Dial::Shutter, 80).label, "1/125");
    }

    /// 1/3 s and 0.4 s both used to print as "1/3", which put two identical
    /// entries in the dropdown.
    #[test]
    fn distinguishes_one_third_of_a_second_from_four_tenths() {
        assert_eq!(exposure_value(Dial::Shutter, 3333).label, "1/3");
        assert_eq!(exposure_value(Dial::Shutter, 4000).label, "0.4s");
    }

    #[test]
    fn bulb_and_time_carry_no_brightness() {
        for raw in [EXPOSURE_TIME_BULB, EXPOSURE_TIME_TIME] {
            let value = exposure_value(Dial::Shutter, raw);
            assert!(value.stops.is_none());
            // The token must round-trip untouched; it is what goes back on a write.
            assert_eq!(value.raw, raw.to_string());
        }
    }

    #[test]
    fn decodes_f_numbers_as_hundredths() {
        let wide = exposure_value(Dial::Aperture, 180);
        assert_eq!(wide.label, "f/1.8");

        let current = exposure_value(Dial::Aperture, 280);
        assert_eq!(current.label, "f/2.8");
        approx(current.stops.unwrap(), -2.0 * 2.8f32.log2());

        assert_eq!(exposure_value(Dial::Aperture, 1600).label, "f/16");
    }

    #[test]
    fn decodes_iso_directly() {
        let value = exposure_value(Dial::Iso, 320);
        assert_eq!(value.label, "320");
        approx(value.stops.unwrap(), (320.0f32 / 100.0).log2());
    }

    /// The real value lists the Z 6 reported, run through the ramp's own
    /// selection logic.
    #[test]
    fn a_ramp_over_the_real_z6_shutter_list_stays_selectable() {
        let raws: [u32; 12] = [
            1000, 1250, 1666, 2000, 2500, 3333, 4000, 5000, 6666, 10000, 13000, 300_000,
        ];
        let values: Vec<_> = raws
            .iter()
            .map(|raw| exposure_value(Dial::Shutter, *raw))
            .collect();

        // From 1/10 s (1000 units), one stop brighter is 1/5 s - 2000 units.
        let start = exposure_value(Dial::Shutter, 1000).stops.unwrap();
        let target = exposure::nearest(&values, start + 1.0).unwrap();
        assert_eq!(target.raw, "2000");

        // Every candidate a ramp can pick has to exist in the camera's own list.
        assert!(raws.contains(&target.raw.parse::<u32>().unwrap()));
    }

    #[test]
    fn a_range_form_yields_no_selectable_values() {
        // Inventing step positions inside a range would produce writes the camera
        // rejects, so we would rather offer nothing.
        let desc = PropDesc {
            property: PROP_EXPOSURE_INDEX,
            datatype: 0x0004,
            writable: true,
            current: super::super::ptpip::PtpValue::U16(320),
            form: Form::Range {
                min: super::super::ptpip::PtpValue::U16(100),
                max: super::super::ptpip::PtpValue::U16(6400),
                step: super::super::ptpip::PtpValue::U16(1),
            },
        };
        assert!(selectable(Dial::Iso, &desc).is_empty());
    }

    fn event(code: u16, params: &[u32]) -> PtpEvent {
        PtpEvent {
            code,
            params: params.to_vec(),
        }
    }

    /// A mapper with a throwaway handle log, for the cases that only care about the
    /// mapping.
    fn mapper() -> EventMapper {
        make_mapper(Arc::new(Mutex::new(VecDeque::new())))
    }

    /// The parameter values are the ones a Z 6 actually sent, in decimal as they
    /// appeared in the log.
    #[test]
    fn maps_dial_changes_to_the_dial_that_moved() {
        assert_eq!(
            mapper()(&event(EVENT_DEVICE_PROP_CHANGED, &[20493])), // 0x500D
            Some(CameraEvent::DialChanged {
                dial: Dial::Shutter
            })
        );
        assert_eq!(
            mapper()(&event(EVENT_DEVICE_PROP_CHANGED, &[20487])), // 0x5007
            Some(CameraEvent::DialChanged {
                dial: Dial::Aperture
            })
        );
        assert_eq!(
            mapper()(&event(EVENT_DEVICE_PROP_CHANGED, &[20495])), // 0x500F
            Some(CameraEvent::DialChanged { dial: Dial::Iso })
        );
    }

    /// Focus chatter is the reason the filter exists: a half-pressed shutter fired
    /// these eleven times in five seconds, and each one would otherwise have cost
    /// three PTP round trips.
    #[test]
    fn ignores_the_focus_chatter() {
        for property in [20490u32, 20508] {
            // 0x500A FocusMode, 0x501C FocusMeteringMode
            assert_eq!(
                mapper()(&event(EVENT_DEVICE_PROP_CHANGED, &[property])),
                None
            );
        }
        // And a property-changed with no parameter must not panic.
        assert_eq!(mapper()(&event(EVENT_DEVICE_PROP_CHANGED, &[])), None);
    }

    /// The handles are what previews are fetched by, so losing them loses previews.
    /// Replays one frame exactly as a Z 6 reported it.
    #[test]
    fn records_written_file_handles_newest_first() {
        let recent = Arc::new(Mutex::new(VecDeque::new()));
        let map = make_mapper(recent.clone());

        map(&event(EVENT_OBJECT_ADDED, &[689539863]));
        map(&event(EVENT_OBJECT_ADDED, &[152668951]));
        map(&event(EVENT_CAPTURE_COMPLETE, &[0]));

        let handles: Vec<u32> = lock(&recent).iter().copied().collect();
        // Newest first: the JPEG is looked for from the most recent end.
        assert_eq!(handles, vec![152668951, 689539863]);
    }

    #[test]
    fn forgets_handles_beyond_the_window() {
        let recent = Arc::new(Mutex::new(VecDeque::new()));
        let map = make_mapper(recent.clone());

        for handle in 1..=(RECENT_HANDLES as u32 + 4) {
            map(&event(EVENT_OBJECT_ADDED, &[handle]));
        }

        let handles: Vec<u32> = lock(&recent).iter().copied().collect();
        assert_eq!(handles.len(), RECENT_HANDLES);
        // The newest survived; the oldest fell off.
        assert_eq!(handles[0], RECENT_HANDLES as u32 + 4);
        assert!(!handles.contains(&1));
    }

    /// One frame per exposure, not one per written file.
    #[test]
    fn counts_frames_by_capture_not_by_file() {
        assert_eq!(
            mapper()(&event(EVENT_CAPTURE_COMPLETE, &[0])),
            Some(CameraEvent::FrameRecorded)
        );
        // RAW+JPEG emits this twice per frame, so it must not become a frame event.
        assert_eq!(mapper()(&event(EVENT_OBJECT_ADDED, &[689539863])), None);
    }

    #[test]
    fn strips_nikons_zero_padded_serial() {
        assert_eq!(
            non_empty("00000000000000000000000006068685".trim_start_matches('0')),
            Some("6068685".to_string())
        );
        // All zeros must not collapse to an empty string masquerading as a serial.
        assert_eq!(non_empty("0000".trim_start_matches('0')), None);
    }
}
