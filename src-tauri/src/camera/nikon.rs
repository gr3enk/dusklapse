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

use async_trait::async_trait;

use super::error::{CameraError, CameraResult};
use super::exposure;
use super::model::{
    BatteryStatus, CameraEvent, CameraInfo, CameraTarget, Dial, ExposureCapabilities,
    ExposureSettings, ExposureValue, Vendor,
};
use super::ptpip::{
    Form, PropDesc, PtpEvent, PtpIp, EVENT_CAPTURE_COMPLETE, EVENT_DEVICE_PROP_CHANGED,
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
fn map_event(event: &PtpEvent) -> Option<CameraEvent> {
    match event.code {
        EVENT_CAPTURE_COMPLETE => Some(CameraEvent::FrameRecorded),
        EVENT_DEVICE_PROP_CHANGED => {
            let property = *event.params.first()? as u16;
            property_dial(property).map(|dial| CameraEvent::DialChanged { dial })
        }
        _ => None,
    }
}

pub struct NikonPtpIp {
    target: CameraTarget,
    session: PtpIp,
    info: CameraInfo,
}

impl NikonPtpIp {
    pub async fn connect(target: CameraTarget) -> CameraResult<Self> {
        let session = PtpIp::connect(&target.host, target.port, CLIENT_NAME, map_event).await?;

        let device = session.device_info();
        let info = CameraInfo {
            vendor: Vendor::Nikon,
            manufacturer: non_empty(&device.manufacturer)
                .unwrap_or_else(|| "Nikon".to_string()),
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
        let raw: u32 = value
            .parse()
            .map_err(|_| CameraError::ValueNotSelectable {
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

    fn events(&self) -> Option<tokio::sync::broadcast::Receiver<CameraEvent>> {
        Some(self.session.subscribe_camera())
    }

    async fn disconnect(&self) -> CameraResult<()> {
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
    use crate::camera::ptpip::EVENT_OBJECT_ADDED;

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-3, "{a} != {b}");
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

    /// The parameter values are the ones a Z 6 actually sent, in decimal as they
    /// appeared in the log.
    #[test]
    fn maps_dial_changes_to_the_dial_that_moved() {
        assert_eq!(
            map_event(&event(EVENT_DEVICE_PROP_CHANGED, &[20493])), // 0x500D
            Some(CameraEvent::DialChanged {
                dial: Dial::Shutter
            })
        );
        assert_eq!(
            map_event(&event(EVENT_DEVICE_PROP_CHANGED, &[20487])), // 0x5007
            Some(CameraEvent::DialChanged {
                dial: Dial::Aperture
            })
        );
        assert_eq!(
            map_event(&event(EVENT_DEVICE_PROP_CHANGED, &[20495])), // 0x500F
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
            assert_eq!(map_event(&event(EVENT_DEVICE_PROP_CHANGED, &[property])), None);
        }
        // And a property-changed with no parameter must not panic.
        assert_eq!(map_event(&event(EVENT_DEVICE_PROP_CHANGED, &[])), None);
    }

    /// One frame per exposure, not one per written file.
    #[test]
    fn counts_frames_by_capture_not_by_file() {
        assert_eq!(
            map_event(&event(EVENT_CAPTURE_COMPLETE, &[0])),
            Some(CameraEvent::FrameRecorded)
        );
        // RAW+JPEG emits this twice per frame, so it must not become a frame event.
        assert_eq!(map_event(&event(EVENT_OBJECT_ADDED, &[689539863])), None);
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
