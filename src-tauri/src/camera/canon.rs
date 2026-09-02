//! Canon CCAPI backend.
//!
//! CCAPI is a REST/JSON API the camera itself serves over Wi-Fi, which makes it
//! by far the friendliest of the three vendors. Two things to know before
//! debugging against a real body:
//!
//! 1. CCAPI ships disabled. It has to be unlocked per camera through Canon's
//!    developer registration, which hands out a per-body activation file. A
//!    camera that has not been unlocked simply does not answer on port 8080 -
//!    that looks identical to a wrong IP from here.
//! 2. Endpoint paths are versioned (`ver100`, `ver110`, … `ver130`) and which
//!    versions exist differs per body and per firmware. So we do not hardcode a
//!    version: we ask the camera what it offers and resolve each endpoint
//!    against that listing, preferring the newest version it advertises.
//!
//! The concrete request and response shapes below follow Canon's CCAPI
//! reference. If a body rejects something, check its own `/ccapi` listing first
//! - that is the authority for what it supports, not this file.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use super::error::{CameraError, CameraResult};
use super::model::{
    BatteryStatus, CameraEvent, CameraInfo, CameraTarget, Dial, ExposureCapabilities,
    ExposureSettings, ExposureValue, Preview, Vendor, VendorProfile,
};
use super::Camera;

/// Cameras answer fast on a local network; a long timeout just hides a body that
/// has dropped off the Wi-Fi behind a stalled UI.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

/// CCAPI's answer for anything it is not offering at the moment.
///
/// Used for a shooting mode that does not expose a dial, and for a dial the body cannot report at
/// all - a manual lens has no f-number to give. Both mean the same thing to this backend: not now.
const UNAVAILABLE: u16 = 503;

const EP_DEVICE_INFO: &str = "deviceinformation";
const EP_BATTERY: &str = "devicestatus/battery";
const EP_SHUTTER: &str = "shooting/control/shutterbutton";
const EP_SHUTTER_MANUAL: &str = "shooting/control/shutterbutton/manual";
/// What the camera has done since this endpoint was last asked, new files included.
const EP_EVENT_POLLING: &str = "event/polling";

/// How often to ask the camera what it has been doing.
///
/// CCAPI has no push channel, so this is the only way a frame is noticed. `timeout=immediately`
/// makes each ask return at once rather than hanging, and a second is short enough that a preview
/// follows the shutter closely without the request being in flight most of the time.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Events dropped rather than queued without bound when nobody is listening.
const EVENT_BUFFER: usize = 32;

/// The rendition to ask for first: the camera's own downsized JPEG.
///
/// A few hundred kilobytes instead of the whole frame, which is all the luminance measurement
/// needs and all a preview pane can show.
const DISPLAY_RENDITION: &str = "display";

/// The rendition to fall back to: the file itself.
///
/// Needed because `display` is not always available. The reference lists the refusals, and one of
/// them is the case a timelapse walks straight into: a JPEG of size S2 or smaller. Someone who
/// picks the smallest JPEG to keep transfers quick is asking Canon to shrink a file it considers
/// already small, and it answers 400 rather than obliging. On such a file `main` is small anyway.
const FULL_RENDITION: &str = "main";

/// Pulling an image is not a settings read.
///
/// The shared timeout is sized for a body that answers in milliseconds; a picture over a camera's
/// own 2.4 GHz radio is a different kind of wait, and cutting it off at the same mark turns a slow
/// frame into a lost session.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(60);

fn dial_endpoint(dial: Dial) -> &'static str {
    match dial {
        Dial::Shutter => "shooting/settings/tv",
        Dial::Aperture => "shooting/settings/av",
        Dial::Iso => "shooting/settings/iso",
    }
}

pub fn profile() -> VendorProfile {
    VendorProfile {
        vendor: Vendor::Canon,
        label: Vendor::Canon.label().to_string(),
        summary: "CCAPI over HTTP - unlock per body, then read the address off the camera".into(),
        default_port: Vendor::Canon.default_port(),
        // Nothing to prefill, and nothing lost by it: the address differs between bodies, and a
        // camera running CCAPI shows its own address and port on screen once it is on a network.
        // That is a better source than any guess here would be.
        access_point_host: None,
        needs_address: true,
        implemented: true,
        developer_only: false,
    }
}

pub struct CanonCcapi {
    target: CameraTarget,
    http: reqwest::Client,
    /// Endpoint suffix -> absolute URL, built from the camera's own listing.
    endpoints: BTreeMap<String, String>,
    info: CameraInfo,
    /// Whether a picture is crossing the network right now.
    ///
    /// The reference is explicit that a content download is unavailable while another one is in
    /// progress, so the watcher gives way rather than competing with it.
    fetching: Arc<AtomicBool>,
    /// Which rendition this camera actually serves, once one has worked.
    ///
    /// Learned rather than configured: the answer depends on the JPEG size someone chose on the
    /// body, so it cannot be known at connect and would be wrong to ask about every frame.
    rendition: Arc<Mutex<&'static str>>,
    /// Path of the newest JPEG the watcher has seen and nobody has fetched yet.
    ///
    /// Filled by the watcher rather than read on demand, because asking the camera what is new
    /// *consumes* the answer - see [`watch`]. One reader, one writer, exactly as the Nikon backend
    /// records object handles as its events go past.
    pending: Arc<Mutex<Option<String>>>,
    events: tokio::sync::broadcast::Sender<CameraEvent>,
    /// The watcher, so it stops when the camera goes away.
    watch: tokio::task::JoinHandle<()>,
}

impl Drop for CanonCcapi {
    fn drop(&mut self) {
        self.watch.abort();
    }
}

impl CanonCcapi {
    pub async fn connect(target: CameraTarget) -> CameraResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|err| CameraError::Transport(err.to_string()))?;

        let root = format!("http://{}:{}/ccapi", target.host, target.port);
        log::info!("probing CCAPI at {root}");

        let listing: BTreeMap<String, Vec<ApiEntry>> = match get_json(&http, &root).await {
            Ok(listing) => listing,
            // A body whose CCAPI has never been unlocked refuses the connection exactly as a wrong
            // address does, and nothing in the reply distinguishes them. Since the unlock is a
            // one-time step people do not know about, and a wrong address is the other half of the
            // answer, the message names both rather than leaving someone to guess.
            Err(CameraError::Transport(reason)) => {
                return Err(CameraError::Transport(format!(
                    "{reason}. Nothing answered at {}:{} - either the address is wrong, or CCAPI \
                     has not been switched on for this camera. CCAPI ships dormant and has to be \
                     unlocked once per body with Canon's activation tool over USB.",
                    target.host, target.port
                )))
            }
            Err(other) => return Err(other),
        };
        if listing.is_empty() {
            return Err(CameraError::Protocol(
                "camera advertised no CCAPI endpoints".into(),
            ));
        }

        let endpoints = resolve_endpoints(&target, &listing);
        report(&listing, &endpoints);
        // BTreeMap keys sort lexicographically, and CCAPI versions are all the
        // same width (`verNNN`), so the last key is the newest.
        let api_version = listing.keys().next_back().cloned();

        let device: DeviceInformation = get_json(
            &http,
            endpoints
                .get(EP_DEVICE_INFO)
                .ok_or_else(|| CameraError::Unavailable("device information".into()))?,
        )
        .await?;

        let info = CameraInfo {
            vendor: Vendor::Canon,
            manufacturer: device.manufacturer,
            model: device.productname,
            serial: device.serialnumber,
            firmware: device.firmwareversion,
            api_version,
            supports_release: true,
            // No push channel of its own, but the watcher below turns its polling endpoint into
            // one, so everything downstream sees the same events a Nikon sends.
            pushes_events: true,
        };
        log::info!("connected to {} {}", info.manufacturer, info.model);

        let (events, _) = tokio::sync::broadcast::channel(EVENT_BUFFER);
        let pending = Arc::new(Mutex::new(None));
        let fetching = Arc::new(AtomicBool::new(false));
        let watch = tokio::spawn(watch(
            http.clone(),
            endpoints.get(EP_EVENT_POLLING).cloned(),
            pending.clone(),
            events.clone(),
            fetching.clone(),
        ));

        let camera = Self {
            target,
            http,
            endpoints,
            info,
            fetching,
            rendition: Arc::new(Mutex::new(DISPLAY_RENDITION)),
            pending,
            events,
            watch,
        };
        // Once, at connect. Which dials a Canon offers depends on the mode dial and on the lens,
        // so it is a fact about this session rather than about the model.
        camera.report_dials().await;

        Ok(camera)
    }

    fn endpoint(&self, suffix: &str) -> CameraResult<&str> {
        self.endpoints
            .get(suffix)
            .map(String::as_str)
            .ok_or_else(|| CameraError::Unavailable(suffix.to_string()))
    }

    /// Read all three dials, in the order they appear on screen.
    ///
    /// One at a time, not together. Asking in parallel looks free and is not: a camera serves one
    /// request at a time, and the R100 answers a burst of three by dropping one of the
    /// connections outright - which arrives as "error sending request" rather than as anything
    /// describing the dial.
    ///
    /// A dial that fails on its own is reported as absent rather than fatal. That covers the case
    /// this app has to live with anyway - a manual lens has no aperture to report, whatever the
    /// refusal looks like - without hiding a camera that has gone away: when *all three* fail,
    /// that is not a dial problem and the error is passed on.
    async fn read_dials(&self) -> CameraResult<[Option<Setting>; 3]> {
        let shutter = self.read_dial(Dial::Shutter).await;
        let aperture = self.read_dial(Dial::Aperture).await;
        let iso = self.read_dial(Dial::Iso).await;

        settled([shutter, aperture, iso])
    }

    /// Read one dial, or `None` where the camera will not talk about it.
    ///
    /// A body answers `Mode not supported` for a dial it is not currently offering, and that is not
    /// always about the mode dial: a lens with no electronic contacts reports no f-number at all,
    /// so on a manual lens the aperture endpoint refuses for as long as that lens is mounted.
    ///
    /// Refusing the whole read because of one dial would make the camera unusable for a case that
    /// works perfectly well - shutter and ISO are still there, and the ramp already handles a dial
    /// with nothing to choose from.
    async fn read_dial(&self, dial: Dial) -> CameraResult<Option<Setting>> {
        match get_json(&self.http, self.endpoint(dial_endpoint(dial))?).await {
            Ok(setting) => Ok(Some(setting)),
            Err(CameraError::Rejected { status, message }) if status == UNAVAILABLE => {
                // Debug, not info: this is read on every poll, and a body that will never offer
                // the dial would fill the log with the same line. The summary at connect is where
                // a person is told once.
                log::debug!("camera will not report {dial:?}: {message}");
                Ok(None)
            }
            Err(err) => Err(err),
        }
    }

    /// Download a picture, working out which rendition this camera will part with.
    ///
    /// `display` is asked for first because it is the small one. A body that will not make a
    /// display copy answers 400 - see [`FULL_RENDITION`] for when that happens - and the file
    /// itself is then the only rendition there is. The working answer is remembered, so the
    /// refusal is paid for once per session rather than once per frame.
    async fn fetch_content(&self, base: &str, filename: &str) -> CameraResult<Vec<u8>> {
        self.fetching.store(true, Ordering::Relaxed);
        let result = self.fetch_rendition(base, filename).await;
        self.fetching.store(false, Ordering::Relaxed);
        result
    }

    async fn fetch_rendition(&self, base: &str, filename: &str) -> CameraResult<Vec<u8>> {
        let wanted = *lock(&self.rendition);
        let request = |kind: &str| {
            self.http
                .get(format!("{base}?kind={kind}"))
                .timeout(TRANSFER_TIMEOUT)
        };

        match send_bytes(request(wanted)).await {
            Ok(bytes) => Ok(bytes),
            Err(CameraError::Rejected { status: 400, .. }) if wanted == DISPLAY_RENDITION => {
                log::info!(
                    "camera will not make a display copy of {filename} - it is already small. \
                     Fetching the file itself from here on."
                );
                *lock(&self.rendition) = FULL_RENDITION;
                send_bytes(request(FULL_RENDITION)).await
            }
            Err(err) => Err(err),
        }
    }

    /// Say once which dials this body and lens combination actually offers.
    async fn report_dials(&self) {
        let Ok(capabilities) = self.capabilities().await else {
            return;
        };

        for (label, values) in [
            ("shutter", &capabilities.shutter),
            ("aperture", &capabilities.aperture),
            ("ISO", &capabilities.iso),
        ] {
            if values.is_empty() {
                log::warn!("{label}: not available on this camera right now");
            } else {
                log::info!("{label}: {} values selectable", values.len());
            }
        }

        if capabilities.aperture.is_empty() {
            log::info!(
                "no aperture to ramp - a lens without electronic contacts reports no f-number. \
                 Shutter and ISO still work."
            );
        }
    }
}

#[async_trait]
impl Camera for CanonCcapi {
    fn target(&self) -> &CameraTarget {
        &self.target
    }

    fn info(&self) -> &CameraInfo {
        &self.info
    }

    async fn capabilities(&self) -> CameraResult<ExposureCapabilities> {
        let [shutter, aperture, iso] = self.read_dials().await?;

        Ok(ExposureCapabilities {
            shutter: values(shutter, Dial::Shutter),
            aperture: values(aperture, Dial::Aperture),
            iso: values(iso, Dial::Iso),
        })
    }

    async fn exposure(&self) -> CameraResult<ExposureSettings> {
        let [shutter, aperture, iso] = self.read_dials().await?;

        Ok(ExposureSettings {
            shutter: current(shutter, Dial::Shutter),
            aperture: current(aperture, Dial::Aperture),
            iso: current(iso, Dial::Iso),
        })
    }

    async fn set_exposure(&self, dial: Dial, value: &str) -> CameraResult<()> {
        let url = self.endpoint(dial_endpoint(dial))?;
        let body = serde_json::json!({ "value": value });
        send(self.http.put(url).json(&body)).await.map(|_| ())
    }

    async fn shoot(&self, autofocus: bool) -> CameraResult<()> {
        let url = self.endpoint(EP_SHUTTER)?;
        let body = serde_json::json!({ "af": autofocus });
        send(self.http.post(url).json(&body)).await.map(|_| ())
    }

    async fn bulb_open(&self) -> CameraResult<()> {
        let url = self.endpoint(EP_SHUTTER_MANUAL)?;
        let body = serde_json::json!({ "af": false, "action": "full_press" });
        send(self.http.post(url).json(&body)).await.map(|_| ())
    }

    async fn bulb_close(&self) -> CameraResult<()> {
        let url = self.endpoint(EP_SHUTTER_MANUAL)?;
        let body = serde_json::json!({ "af": false, "action": "release" });
        send(self.http.post(url).json(&body)).await.map(|_| ())
    }

    async fn battery(&self) -> CameraResult<Option<BatteryStatus>> {
        let url = match self.endpoint(EP_BATTERY) {
            Ok(url) => url,
            // Bodies powered over USB-C or by a grip may not offer this at all.
            Err(CameraError::Unavailable(_)) => return Ok(None),
            Err(err) => return Err(err),
        };
        let battery: BatteryResponse = get_json(&self.http, url).await?;
        Ok(Some(battery.into_status()))
    }

    /// Fetch the newest JPEG the camera has written since the last call.
    ///
    /// Canon announces nothing, so the sequence is: ask what was added, pick the JPEG, download the
    /// camera's own downsized rendition of it.
    ///
    /// Only the JPEG. A body shooting RAW+JPEG writes both, and the small companion file exists for
    /// exactly this purpose - the rule every backend here follows, so a RAW never crosses the
    /// network.
    async fn preview(&self) -> CameraResult<Option<Preview>> {
        // Taken, not peeked: a frame is fetched once. The watcher puts the next one here.
        let Some(path) = lock(&self.pending).take() else {
            return Ok(None);
        };

        let filename = path.rsplit('/').next().unwrap_or(&path).to_string();
        let base = content_url(&self.target, &path);

        log::info!("fetching {filename}");
        let started = std::time::Instant::now();
        let bytes = self.fetch_content(&base, &filename).await?;
        log::info!(
            "fetched {filename} - {} KiB in {:.1}s",
            bytes.len() / 1024,
            started.elapsed().as_secs_f32()
        );

        // Decoded here rather than in the WebView so the curves on screen are the same data the
        // ramp reads. A failure is logged and dropped: the image is still worth showing.
        let analysis = match super::histogram::analyse(&bytes) {
            Ok(analysis) => {
                log::info!(
                    "{filename} measures {} on the brightness scale",
                    analysis.luminance.value
                );
                Some(analysis)
            }
            Err(err) => {
                log::warn!("could not measure {filename}: {err}");
                None
            }
        };

        // From the JPEG's own header: CCAPI's content listing carries paths and nothing else.
        let pixels = super::histogram::dimensions(&bytes).unwrap_or((0, 0));

        Ok(Some(Preview {
            bytes,
            mime: "image/jpeg".into(),
            filename,
            pixels,
            analysis,
        }))
    }

    fn events(&self) -> Option<tokio::sync::broadcast::Receiver<CameraEvent>> {
        Some(self.events.subscribe())
    }

    async fn disconnect(&self) -> CameraResult<()> {
        // Before anything else, so the watcher does not poll a camera we have let go of.
        self.watch.abort();
        // CCAPI is stateless HTTP - there is no session to tear down. Dropping
        // the client is all the cleanup there is.
        Ok(())
    }
}

/// What a dial can be set to, or nothing where the camera would not say.
fn values(setting: Option<Setting>, dial: Dial) -> Vec<ExposureValue> {
    setting.map(|s| s.into_values(dial)).unwrap_or_default()
}

/// What a dial is set to, or nothing where the camera would not say.
fn current(setting: Option<Setting>, dial: Dial) -> Option<ExposureValue> {
    setting.and_then(|s| s.into_current(dial))
}

/// Map every endpoint the camera advertises to an absolute URL, keyed by the
/// part of the path after the version segment.
///
/// Newer versions overwrite older ones because the listing is walked in
/// ascending key order.
fn resolve_endpoints(
    target: &CameraTarget,
    listing: &BTreeMap<String, Vec<ApiEntry>>,
) -> BTreeMap<String, String> {
    let mut endpoints = BTreeMap::new();
    for entries in listing.values() {
        for entry in entries {
            if let Some(suffix) = endpoint_suffix(&entry.path) {
                endpoints.insert(
                    suffix.to_string(),
                    format!("http://{}:{}{}", target.host, target.port, entry.path),
                );
            }
        }
    }
    endpoints
}

/// Write down what the body offers, and which of the things this backend needs it has.
///
/// The whole point of a first connection with a camera nobody here has held before. CCAPI differs
/// per body and per firmware, so a model may simply not serve an endpoint - and the camera's own
/// listing is the authority on that, which beats guessing from a failure three screens later.
fn report(listing: &BTreeMap<String, Vec<ApiEntry>>, endpoints: &BTreeMap<String, String>) {
    log::info!(
        "camera serves CCAPI {} with {} endpoint(s)",
        listing.keys().cloned().collect::<Vec<_>>().join(", "),
        endpoints.len()
    );

    // Named one by one rather than left to the reader: scanning a hundred paths by eye for the one
    // that is missing is how a wrong conclusion gets drawn.
    for (label, suffix) in [
        ("device information", EP_DEVICE_INFO),
        ("battery", EP_BATTERY),
        ("shutter button", EP_SHUTTER),
        ("shutter button (manual)", EP_SHUTTER_MANUAL),
        ("new files (event polling)", EP_EVENT_POLLING),
        ("shutter speed", dial_endpoint(Dial::Shutter)),
        ("aperture", dial_endpoint(Dial::Aperture)),
        ("ISO", dial_endpoint(Dial::Iso)),
    ] {
        match endpoints.get(suffix) {
            Some(url) => log::info!("  {label}: {url}"),
            None => log::warn!("  {label}: NOT offered ({suffix})"),
        }
    }

    // Every other endpoint, on one line. CCAPI differs per body and firmware, so this listing is
    // the map for anything this backend does not reach yet - and one line is cheap enough to keep
    // at info, where a person can actually see it.
    let known = [
        EP_DEVICE_INFO,
        EP_BATTERY,
        EP_SHUTTER,
        EP_SHUTTER_MANUAL,
        EP_EVENT_POLLING,
        dial_endpoint(Dial::Shutter),
        dial_endpoint(Dial::Aperture),
        dial_endpoint(Dial::Iso),
    ];
    let rest: Vec<&str> = endpoints
        .keys()
        .map(String::as_str)
        .filter(|suffix| !known.contains(suffix))
        .collect();
    log::info!("also offers: {}", rest.join(" "));
}

/// `/ccapi/ver100/shooting/settings/tv` -> `shooting/settings/tv`
fn endpoint_suffix(path: &str) -> Option<&str> {
    let rest = path.trim_start_matches('/').strip_prefix("ccapi/")?;
    let (_version, suffix) = rest.split_once('/')?;
    (!suffix.is_empty()).then_some(suffix)
}

async fn get_json<T: serde::de::DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
) -> CameraResult<T> {
    let body = send(http.get(url)).await?;
    serde_json::from_str(&body)
        .map_err(|err| CameraError::Protocol(format!("could not read reply from {url}: {err}")))
}

/// Run a request and turn a non-2xx status into a `Rejected` carrying whatever
/// explanation the camera gave.
/// The same as [`send`], for a response that is an image rather than JSON.
async fn send_bytes(request: reqwest::RequestBuilder) -> CameraResult<Vec<u8>> {
    let response = request.send().await?;
    let status = response.status();
    if !status.is_success() {
        return Err(CameraError::Rejected {
            status: status.as_u16(),
            message: response.text().await.unwrap_or_default(),
        });
    }
    Ok(response.bytes().await?.to_vec())
}

async fn send(request: reqwest::RequestBuilder) -> CameraResult<String> {
    let response = request.send().await?;
    let status = response.status();
    let body = response.text().await?;

    if status.is_success() {
        return Ok(body);
    }

    // CCAPI puts a human-readable reason in `message` on failures.
    let message = serde_json::from_str::<ErrorResponse>(&body)
        .map(|error| error.message)
        .unwrap_or_else(|_| {
            if body.trim().is_empty() {
                status.to_string()
            } else {
                body.trim().to_string()
            }
        });

    Err(CameraError::Rejected {
        status: status.as_u16(),
        message: explain(status.as_u16(), message),
    })
}

/// Add the cause behind a CCAPI refusal that would otherwise say nothing useful.
///
/// `Mode not supported` is the one worth catching. It does not mean the camera dislikes the value
/// or the endpoint - it means the mode dial is somewhere that does not let a person set that dial
/// either, so the camera will not let this app set it. On an EOS R100 that is A+, Hybrid Auto, SCN
/// and the creative filters; the exposure settings only exist in P, Tv, Av and M.
///
/// Left alone otherwise. Canon's own wording is usually the clearest thing available, and wrapping
/// every message in guesses would bury the ones that are already precise.
fn explain(status: u16, message: String) -> String {
    if status == UNAVAILABLE && message.to_lowercase().contains("mode") {
        return format!(
            "{message}. The camera's mode dial is in a position that does not expose shutter, \
             aperture and ISO - set it to M."
        );
    }
    message
}

#[derive(Deserialize)]
struct ApiEntry {
    path: String,
}

#[derive(Deserialize)]
struct ErrorResponse {
    message: String,
}

#[derive(Deserialize)]
struct DeviceInformation {
    manufacturer: String,
    productname: String,
    #[serde(default)]
    serialnumber: Option<String>,
    #[serde(default)]
    firmwareversion: Option<String>,
}

/// Shape of every `shooting/settings/*` endpoint: the value in effect plus the
/// values currently selectable.
/// Turn Canon's polling endpoint into the event channel it does not have.
///
/// CCAPI announces nothing on its own; the camera has to be asked what it has been doing, and each
/// answer covers only the time since the previous one. That makes the endpoint a drain with room
/// for exactly one reader, which is why this task owns it and everything else reads what it leaves
/// behind - the same arrangement the Nikon backend uses for object handles.
///
/// The alternative, letting `preview` ask directly, was tried first and cannot work: nothing calls
/// `preview` until a frame has been announced, and nothing announces a frame until someone asks.
async fn watch(
    http: reqwest::Client,
    url: Option<String>,
    pending: Arc<Mutex<Option<String>>>,
    events: tokio::sync::broadcast::Sender<CameraEvent>,
    fetching: Arc<AtomicBool>,
) {
    let Some(url) = url else {
        log::warn!("camera does not offer {EP_EVENT_POLLING}; no frames will be reported");
        return;
    };
    let url = polling_url(&url);
    // Named once, because the version this resolved to decides both how to ask and what comes
    // back: a body speaking CCAPI 1.0.0 takes `continue` and lists whole URLs in `addedcontents`,
    // one speaking 1.1.0 or later takes `timeout` and lists paths. It is also the proof that this
    // task is running at all.
    log::info!("watching {url} for new frames");
    let mut described = false;
    let mut complained = false;

    // The first answer is not a report of what changed; it is the camera describing itself from
    // scratch, contents included. Measured on an EOS R100: thirty-three files already on the card
    // arrived in it, which would have counted as thirty-three frames. So the first answer settles
    // what is already there and announces none of it - the same baseline the Nikon card watch
    // takes for the same reason.
    let mut baseline = true;

    loop {
        tokio::time::sleep(POLL_INTERVAL).await;

        // Give way to a picture already crossing: the reference says a content download is
        // unavailable while another is in progress, and a poll in the middle of one is a request
        // the camera has to turn away.
        if fetching.load(Ordering::Relaxed) {
            continue;
        }

        let raw = match send(http.get(&url)).await {
            Ok(raw) => raw,
            // A single failed poll is not worth ending the watch over - the camera may be busy
            // writing. But a poll that fails *every* time is why no frame is ever noticed, and
            // hiding that at debug is how this went unexplained for a whole evening. Said once,
            // loudly, then quietly.
            Err(err) => {
                if complained {
                    log::debug!("could not ask the camera what changed: {err}");
                } else {
                    complained = true;
                    log::warn!("could not ask the camera what changed: {err}");
                }
                continue;
            }
        };

        let Ok(answer) = serde_json::from_str::<PollingResponse>(&raw) else {
            log::debug!("unreadable answer from {EP_EVENT_POLLING}: {raw}");
            continue;
        };
        if answer.is_quiet() {
            continue;
        }

        // Once, the first time the camera says anything: the shape of this answer is what a
        // future change to this backend is written against, and no document is as reliable.
        if !described {
            described = true;
            log::info!("first change reported by the camera: {raw}");
        }

        if baseline {
            baseline = false;
            if !answer.addedcontents.is_empty() {
                log::info!(
                    "{} file(s) already on the card; counting from here",
                    answer.addedcontents.len()
                );
            }
        } else {
            for frame in frames_in(&answer.addedcontents) {
                if let Some(jpeg) = frame.jpeg {
                    *lock(&pending) = Some(jpeg);
                }
                // Err only means nobody is listening, which is fine.
                let _ = events.send(CameraEvent::FrameRecorded);
            }
        }

        for dial in answer.changed_dials() {
            let _ = events.send(CameraEvent::DialChanged { dial });
        }
    }
}

/// One frame, and the JPEG belonging to it if the camera wrote one.
struct Frame {
    jpeg: Option<String>,
}

/// Group added files into the frames that produced them.
///
/// A body shooting RAW+JPEG writes two files per exposure, and counting files would run the ramp
/// at double speed - the same trap the Nikon backend avoids by counting captures rather than
/// objects. Canon has no capture event, but its filenames do the grouping: `IMG_0042.CR3` and
/// `IMG_0042.JPG` are one frame.
fn frames_in(paths: &[String]) -> Vec<Frame> {
    let mut frames: Vec<(String, Frame)> = Vec::new();

    for path in paths {
        let stem = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .rsplit_once('.')
            .map(|(stem, _)| stem)
            .unwrap_or(path)
            .to_string();

        let jpeg = is_jpeg_path(path).then(|| path.clone());
        match frames.iter_mut().find(|(name, _)| *name == stem) {
            // The JPEG half of a pair already seen as a RAW.
            Some((_, frame)) => frame.jpeg = frame.jpeg.take().or(jpeg),
            None => frames.push((stem, Frame { jpeg })),
        }
    }

    frames.into_iter().map(|(_, frame)| frame).collect()
}

/// A poisoned lock here means a thread panicked while noting a filename. The slot still holds a
/// valid path or nothing, and giving up on previews for the rest of the session would be worse.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// How to ask this camera for events, which differs by the version the endpoint resolved to.
///
/// CCAPI 1.0.0 takes `continue`, where the default of `off` already means "answer now"; 1.1.0 and
/// later take `timeout`, whose default is the same but which is named explicitly so the intent is
/// visible on the wire. The two are not interchangeable - sending `timeout` to a 1.0.0 endpoint is
/// an unknown parameter, and the camera refuses the whole request.
fn polling_url(base: &str) -> String {
    if base.contains("/ver100/") {
        // No parameter at all: `continue=off` is the default and adding it says nothing.
        base.to_string()
    } else {
        format!("{base}?timeout=immediately")
    }
}

/// Decide what three dial reads amount to.
///
/// A dial nobody can read is absent, which the rest of the app already copes with - a manual lens
/// leaves the aperture that way for as long as it is mounted. All three failing at once is
/// something else: that is the camera rather than a dial, and the caller has to hear about it.
fn settled(results: [CameraResult<Option<Setting>>; 3]) -> CameraResult<[Option<Setting>; 3]> {
    let total = results.len();
    let mut settings: [Option<Setting>; 3] = [None, None, None];
    let mut failure = None;
    let mut failed = 0;

    for (slot, result) in results.into_iter().enumerate() {
        match result {
            Ok(setting) => settings[slot] = setting,
            Err(err) => {
                // Debug, not info: this runs on every poll, and a body that will never offer the
                // dial would repeat it forever. The summary at connect says it once.
                log::debug!("could not read a dial: {err}");
                failed += 1;
                failure.get_or_insert(err);
            }
        }
    }

    match failure {
        Some(err) if failed == total => Err(err),
        _ => Ok(settings),
    }
}

/// Turn an entry from `addedcontents` into something fetchable.
///
/// The reference is explicit that the two shapes exist: a body speaking CCAPI 1.0.0 lists whole
/// URLs there, one speaking 1.1.0 or later lists paths. Which one arrives depends on the version
/// the camera resolved that endpoint to, so both are accepted rather than one assumed - prefixing
/// a URL that already has a host produces an address that cannot resolve, silently.
fn content_url(target: &CameraTarget, entry: &str) -> String {
    if entry.starts_with("http://") || entry.starts_with("https://") {
        return entry.to_string();
    }
    format!("http://{}:{}{entry}", target.host, target.port)
}

/// Whether a content path names a JPEG, by its extension.
///
/// The listing gives paths, not formats, so the extension is all there is to go on. `.JPG` on a
/// Canon, upper case in practice, but matched either way.
fn is_jpeg_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".jpg") || lower.ends_with(".jpeg")
}

/// What the camera has done since it was last asked.
///
/// Only the parts this backend acts on are named; CCAPI reports far more, and a body sends
/// whichever of them changed. Everything else is ignored rather than rejected, so a firmware that
/// adds a field does not break the parse.
#[derive(Deserialize)]
struct PollingResponse {
    #[serde(default)]
    addedcontents: Vec<String>,
    #[serde(default)]
    tv: Option<serde_json::Value>,
    #[serde(default)]
    av: Option<serde_json::Value>,
    #[serde(default)]
    iso: Option<serde_json::Value>,
}

impl PollingResponse {
    /// Whether the camera reported nothing this backend cares about.
    fn is_quiet(&self) -> bool {
        self.addedcontents.is_empty() && self.changed_dials().is_empty()
    }

    /// The dials the camera says have moved, so the app can re-read exactly those.
    ///
    /// This is what keeps a Canon as responsive as a Nikon despite having no push channel: a ring
    /// turned on the body shows up within the poll interval rather than at the next heartbeat.
    fn changed_dials(&self) -> Vec<Dial> {
        [
            (Dial::Shutter, &self.tv),
            (Dial::Aperture, &self.av),
            (Dial::Iso, &self.iso),
        ]
        .into_iter()
        .filter(|(_, reported)| reported.is_some())
        .map(|(dial, _)| dial)
        .collect()
    }
}

#[derive(Clone, Deserialize)]
struct Setting {
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    ability: Vec<String>,
}

impl Setting {
    fn into_values(self, dial: Dial) -> Vec<ExposureValue> {
        self.ability
            .into_iter()
            .map(|raw| ExposureValue::from_raw(dial, raw))
            .collect()
    }

    fn into_current(self, dial: Dial) -> Option<ExposureValue> {
        self.value
            .filter(|raw| !raw.is_empty())
            .map(|raw| ExposureValue::from_raw(dial, raw))
    }
}

#[derive(Deserialize)]
struct BatteryResponse {
    /// Either a keyword (`full`, `half`, `low`, …) or a numeric percentage as a
    /// string, depending on the body.
    level: String,
}

impl BatteryResponse {
    fn into_status(self) -> BatteryStatus {
        let percent = match self.level.trim().to_ascii_lowercase().as_str() {
            "full" => Some(100),
            "high" => Some(75),
            "half" => Some(50),
            "quarter" => Some(25),
            "low" => Some(10),
            "end" => Some(0),
            numeric => numeric.parse::<u8>().ok(),
        };
        BatteryStatus {
            percent,
            label: self.level,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The refusal a body in A+ or a scene mode gives, which reads as a fault in the app.
    /// The shutter list an EOS R100 actually sends, firmware 1.3.0.
    ///
    /// Kept verbatim because Canon's on-camera notation is unusual in two ways: the double quote
    /// marks seconds *and* doubles as the decimal point, so `3"2` is 3.2 seconds and not 32.
    /// A body shooting RAW+JPEG writes both files, and only the small one may cross the network.
    /// CCAPI 1.0.0 lists whole URLs, 1.1.0 and later list paths. Prefixing the first kind
    /// produces an address that cannot resolve, and nothing says so - the fetch just fails.
    /// The two versions take different parameters, and the wrong one makes the camera refuse the
    /// request outright - which looks exactly like a camera that never takes a picture.
    fn a_setting() -> Setting {
        Setting {
            value: Some("1/250".into()),
            ability: vec!["1/250".into()],
        }
    }

    fn refused() -> CameraError {
        CameraError::Rejected {
            status: 503,
            message: "Mode not supported".into(),
        }
    }

    /// A manual lens leaves the aperture unreadable for as long as it is mounted. Losing the other
    /// two dials over it would make the camera useless for a case that works perfectly well.
    #[test]
    fn one_unreadable_dial_does_not_cost_the_others() {
        let settled = settled([Ok(Some(a_setting())), Err(refused()), Ok(Some(a_setting()))])
            .expect("two working dials are still a usable camera");

        assert!(settled[0].is_some(), "shutter survives");
        assert!(settled[1].is_none(), "aperture is simply absent");
        assert!(settled[2].is_some(), "ISO survives");
    }

    /// All three failing is not a dial problem, and showing empty dials would hide a camera that
    /// has gone off the network.
    #[test]
    fn a_camera_that_answers_nothing_is_an_error() {
        assert!(settled([Err(refused()), Err(refused()), Err(refused())]).is_err());
    }

    #[test]
    fn three_working_dials_come_back_whole() {
        let settled = settled([
            Ok(Some(a_setting())),
            Ok(Some(a_setting())),
            Ok(Some(a_setting())),
        ])
        .unwrap();
        assert!(settled.iter().all(Option::is_some));
    }

    #[test]
    fn each_ccapi_version_is_asked_the_way_it_expects() {
        // 1.0.0 takes `continue`, whose default already means answer now.
        assert_eq!(
            polling_url("http://192.168.1.2:8080/ccapi/ver100/event/polling"),
            "http://192.168.1.2:8080/ccapi/ver100/event/polling"
        );
        // 1.1.0 and later take `timeout`.
        assert_eq!(
            polling_url("http://192.168.1.2:8080/ccapi/ver110/event/polling"),
            "http://192.168.1.2:8080/ccapi/ver110/event/polling?timeout=immediately"
        );
        assert_eq!(
            polling_url("http://192.168.1.2:8080/ccapi/ver130/event/polling"),
            "http://192.168.1.2:8080/ccapi/ver130/event/polling?timeout=immediately"
        );
    }

    #[test]
    fn both_shapes_of_content_entry_become_one_url() {
        let target = CameraTarget::new(Vendor::Canon, "192.168.1.2", 8080);
        let expected = "http://192.168.1.2:8080/ccapi/ver100/contents/sd/100CANON/IMG_0002.JPG";

        // A path, as ver1.1.0 and later report it.
        assert_eq!(
            content_url(&target, "/ccapi/ver100/contents/sd/100CANON/IMG_0002.JPG"),
            expected
        );
        // A whole URL, as ver1.0.0 reports it - left exactly as it came.
        assert_eq!(content_url(&target, expected), expected);
    }

    #[test]
    fn only_jpeg_paths_are_fetched() {
        assert!(is_jpeg_path(
            "/ccapi/ver110/contents/sd/100CANON/IMG_0042.JPG"
        ));
        assert!(is_jpeg_path(
            "/ccapi/ver110/contents/sd/100CANON/IMG_0042.jpeg"
        ));
        // The RAW companion, which must never be pulled.
        assert!(!is_jpeg_path(
            "/ccapi/ver110/contents/sd/100CANON/IMG_0042.CR3"
        ));
        assert!(!is_jpeg_path(
            "/ccapi/ver110/contents/sd/100CANON/MVI_0042.MP4"
        ));
    }

    /// RAW+JPEG writes two files for one exposure. Counting files would run the ramp at double
    /// speed, so the two are grouped back into the frame they came from.
    #[test]
    fn a_raw_and_its_jpeg_are_one_frame() {
        let added = [
            "/ccapi/ver110/contents/sd/100CANON/IMG_0042.CR3".to_string(),
            "/ccapi/ver110/contents/sd/100CANON/IMG_0042.JPG".to_string(),
        ];

        let frames = frames_in(&added);
        assert_eq!(frames.len(), 1, "one exposure, one frame");
        assert_eq!(
            frames[0].jpeg.as_deref(),
            Some("/ccapi/ver110/contents/sd/100CANON/IMG_0042.JPG"),
            "the small file is the one to fetch"
        );
    }

    /// Two exposures can land in one answer when they fall inside the same poll.
    #[test]
    fn separate_exposures_stay_separate_frames() {
        let added = [
            "/ccapi/ver110/contents/sd/100CANON/IMG_0042.JPG".to_string(),
            "/ccapi/ver110/contents/sd/100CANON/IMG_0043.JPG".to_string(),
        ];
        assert_eq!(frames_in(&added).len(), 2);
    }

    /// Shooting RAW alone is still a frame; there is simply nothing to measure it with.
    #[test]
    fn a_raw_only_frame_counts_but_offers_no_preview() {
        let added = ["/ccapi/ver110/contents/sd/100CANON/IMG_0042.CR3".to_string()];

        let frames = frames_in(&added);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].jpeg.is_none());
    }

    /// A ring turned on the body must reach the app, or the strip shows a stale value for as long
    /// as the heartbeat takes.
    #[test]
    fn a_changed_dial_is_reported() {
        let answer: PollingResponse =
            serde_json::from_str(r#"{"tv":{"value":"1/125"},"iso":{"value":"400"}}"#).unwrap();

        assert!(!answer.is_quiet());
        assert_eq!(answer.changed_dials(), vec![Dial::Shutter, Dial::Iso]);
    }

    /// Between frames the camera answers with nothing to report, which is not an event.
    #[test]
    fn an_empty_answer_is_quiet() {
        let answer: PollingResponse = serde_json::from_str("{}").unwrap();
        assert!(answer.is_quiet());
        assert!(answer.changed_dials().is_empty());
    }

    #[test]
    fn the_r100_shutter_list_is_read_correctly() {
        let setting = Setting {
            value: Some("1/250".into()),
            ability: ["bulb", "30\"", "3\"2", "0\"8", "1\"", "1/250", "1/4000"]
                .map(String::from)
                .to_vec(),
        };

        let values = setting.clone().into_values(Dial::Shutter);
        // Recovered through log2 and back, so compared with a tolerance rather than exactly.
        let is = |raw: &str, expected: f32| {
            let seconds = values
                .iter()
                .find(|value| value.raw == raw)
                .unwrap_or_else(|| panic!("{raw} missing"))
                .stops
                .map(f32::exp2)
                .unwrap_or_else(|| panic!("{raw} carries no stop position"));
            assert!(
                (seconds - expected).abs() < expected * 1e-4,
                "{raw} read as {seconds}s, expected {expected}s"
            );
        };

        is("30\"", 30.0);
        is("1\"", 1.0);
        // The quote is the decimal point here, not a seconds marker on its own.
        is("3\"2", 3.2);
        is("0\"8", 0.8);
        is("1/250", 1.0 / 250.0);

        // Bulb has no fixed duration, so the ramp must never pick it.
        let bulb = values.iter().find(|value| value.raw == "bulb").unwrap();
        assert!(bulb.stops.is_none());

        assert_eq!(setting.into_current(Dial::Shutter).unwrap().raw, "1/250");
    }

    /// The ISO list from the same body. `auto` carries no sensitivity to ramp on.
    #[test]
    fn the_r100_iso_list_is_read_correctly() {
        let setting = Setting {
            value: Some("200".into()),
            ability: ["auto", "100", "200", "12800"].map(String::from).to_vec(),
        };

        let values = setting.clone().into_values(Dial::Iso);
        assert_eq!(values.len(), 4);
        assert!(values[0].stops.is_none(), "auto has no stop position");
        assert!(values[1].stops.is_some());

        assert_eq!(setting.into_current(Dial::Iso).unwrap().raw, "200");
    }

    /// A manual lens leaves the camera with no f-number to report, and the aperture endpoint
    /// refuses for as long as it is mounted. That must cost the other two dials nothing.
    #[test]
    fn a_refused_dial_yields_nothing_rather_than_failing() {
        assert!(values(None, Dial::Aperture).is_empty());
        assert!(current(None, Dial::Aperture).is_none());
    }

    #[test]
    fn a_mode_refusal_says_what_to_do_about_it() {
        let explained = explain(503, "Mode not supported".into());
        assert!(explained.starts_with("Mode not supported"), "{explained}");
        assert!(explained.contains("set it to M"), "{explained}");
    }

    /// Canon's own wording is usually the clearest thing available; only the one case is wrapped.
    #[test]
    fn other_refusals_are_passed_through_untouched() {
        assert_eq!(
            explain(400, "Invalid parameter".into()),
            "Invalid parameter"
        );
        // A 503 about something other than the mode is not this problem.
        assert_eq!(explain(503, "Device busy".into()), "Device busy");
    }

    #[test]
    fn endpoint_suffix_strips_the_version_segment() {
        assert_eq!(
            endpoint_suffix("/ccapi/ver100/shooting/settings/tv"),
            Some("shooting/settings/tv")
        );
        assert_eq!(
            endpoint_suffix("/ccapi/ver130/deviceinformation"),
            Some("deviceinformation")
        );
        // Not a CCAPI path, or nothing left after the version.
        assert_eq!(endpoint_suffix("/other/ver100/thing"), None);
        assert_eq!(endpoint_suffix("/ccapi/ver100"), None);
    }

    #[test]
    fn newer_api_versions_win() {
        let listing = BTreeMap::from([
            (
                "ver100".to_string(),
                vec![ApiEntry {
                    path: "/ccapi/ver100/shooting/settings/iso".into(),
                }],
            ),
            (
                "ver130".to_string(),
                vec![ApiEntry {
                    path: "/ccapi/ver130/shooting/settings/iso".into(),
                }],
            ),
        ]);

        let target = CameraTarget::new(Vendor::Canon, "192.168.1.2", 8080);
        let endpoints = resolve_endpoints(&target, &listing);

        assert_eq!(
            endpoints.get("shooting/settings/iso").map(String::as_str),
            Some("http://192.168.1.2:8080/ccapi/ver130/shooting/settings/iso")
        );
    }

    #[test]
    fn battery_keywords_and_percentages_both_work() {
        let keyword = BatteryResponse {
            level: "half".into(),
        }
        .into_status();
        assert_eq!(keyword.percent, Some(50));

        let numeric = BatteryResponse { level: "63".into() }.into_status();
        assert_eq!(numeric.percent, Some(63));

        let unknown = BatteryResponse {
            level: "unknown".into(),
        }
        .into_status();
        assert_eq!(unknown.percent, None);
        assert_eq!(unknown.label, "unknown");
    }

    #[test]
    fn a_setting_reply_becomes_dial_values() {
        let reply: Setting =
            serde_json::from_str(r#"{"value":"1/125","ability":["1/250","1/125","1/60","bulb"]}"#)
                .unwrap();

        let values = reply.into_values(Dial::Shutter);
        assert_eq!(values.len(), 4);
        assert_eq!(values[1].raw, "1/125");
        // `bulb` is carried through but has no stop position, so ramping skips it.
        assert!(values[3].stops.is_none());
    }
}
