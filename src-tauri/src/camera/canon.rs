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
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;

use super::error::{CameraError, CameraResult};
use super::model::{
    BatteryStatus, CameraInfo, CameraTarget, Dial, ExposureCapabilities, ExposureSettings,
    ExposureValue, Vendor,
};
use super::Camera;

/// Cameras answer fast on a local network; a long timeout just hides a body that
/// has dropped off the Wi-Fi behind a stalled UI.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);

const EP_DEVICE_INFO: &str = "deviceinformation";
const EP_BATTERY: &str = "devicestatus/battery";
const EP_SHUTTER: &str = "shooting/control/shutterbutton";
const EP_SHUTTER_MANUAL: &str = "shooting/control/shutterbutton/manual";

fn dial_endpoint(dial: Dial) -> &'static str {
    match dial {
        Dial::Shutter => "shooting/settings/tv",
        Dial::Aperture => "shooting/settings/av",
        Dial::Iso => "shooting/settings/iso",
    }
}

pub struct CanonCcapi {
    target: CameraTarget,
    http: reqwest::Client,
    /// Endpoint suffix -> absolute URL, built from the camera's own listing.
    endpoints: BTreeMap<String, String>,
    info: CameraInfo,
}

impl CanonCcapi {
    pub async fn connect(target: CameraTarget) -> CameraResult<Self> {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|err| CameraError::Transport(err.to_string()))?;

        let root = format!("http://{}:{}/ccapi", target.host, target.port);
        log::info!("probing CCAPI at {root}");

        let listing: BTreeMap<String, Vec<ApiEntry>> = get_json(&http, &root).await?;
        if listing.is_empty() {
            return Err(CameraError::Protocol(
                "camera advertised no CCAPI endpoints".into(),
            ));
        }

        let endpoints = resolve_endpoints(&target, &listing);
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
            // CCAPI has an event-polling endpoint but no push channel.
            pushes_events: false,
        };
        log::info!("connected to {} {}", info.manufacturer, info.model);

        Ok(Self {
            target,
            http,
            endpoints,
            info,
        })
    }

    fn endpoint(&self, suffix: &str) -> CameraResult<&str> {
        self.endpoints
            .get(suffix)
            .map(String::as_str)
            .ok_or_else(|| CameraError::Unavailable(suffix.to_string()))
    }

    async fn read_dial(&self, dial: Dial) -> CameraResult<Setting> {
        get_json(&self.http, self.endpoint(dial_endpoint(dial))?).await
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
        // Three independent round trips; no reason to pay for them serially.
        let (shutter, aperture, iso) = tokio::try_join!(
            self.read_dial(Dial::Shutter),
            self.read_dial(Dial::Aperture),
            self.read_dial(Dial::Iso),
        )?;

        Ok(ExposureCapabilities {
            shutter: shutter.into_values(Dial::Shutter),
            aperture: aperture.into_values(Dial::Aperture),
            iso: iso.into_values(Dial::Iso),
        })
    }

    async fn exposure(&self) -> CameraResult<ExposureSettings> {
        let (shutter, aperture, iso) = tokio::try_join!(
            self.read_dial(Dial::Shutter),
            self.read_dial(Dial::Aperture),
            self.read_dial(Dial::Iso),
        )?;

        Ok(ExposureSettings {
            shutter: shutter.into_current(Dial::Shutter),
            aperture: aperture.into_current(Dial::Aperture),
            iso: iso.into_current(Dial::Iso),
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

    async fn disconnect(&self) -> CameraResult<()> {
        // CCAPI is stateless HTTP - there is no session to tear down. Dropping
        // the client is all the cleanup there is.
        Ok(())
    }
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
    serde_json::from_str(&body).map_err(|err| {
        CameraError::Protocol(format!("could not read reply from {url}: {err}"))
    })
}

/// Run a request and turn a non-2xx status into a `Rejected` carrying whatever
/// explanation the camera gave.
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
        message,
    })
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
#[derive(Deserialize)]
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

        let numeric = BatteryResponse {
            level: "63".into(),
        }
        .into_status();
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
        let reply: Setting = serde_json::from_str(
            r#"{"value":"1/125","ability":["1/250","1/125","1/60","bulb"]}"#,
        )
        .unwrap();

        let values = reply.into_values(Dial::Shutter);
        assert_eq!(values.len(), 4);
        assert_eq!(values[1].raw, "1/125");
        // `bulb` is carried through but has no stop position, so ramping skips it.
        assert!(values[3].stops.is_none());
    }
}
