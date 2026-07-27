//! Exposure arithmetic in stop space.
//!
//! This is the piece the holy-grail ramp is built on. Cameras only offer
//! discrete values on each dial, and those values differ per body, per lens and
//! per shooting mode - so a ramp cannot work in "shutter speeds", it has to work
//! in stops and then snap onto whatever the camera actually offers.
//!
//! The convention throughout: **stops measure brightness, positive is
//! brighter.** A slower shutter, a wider aperture and a higher ISO all move the
//! number up. That means total brightness is just the sum of the three dials,
//! and a ramp of "+1 stop over the next 40 frames" is plain addition.

use super::model::{Dial, ExposureSettings, ExposureValue};

/// ISO the stop scale is anchored to. ISO 100 contributes 0 stops.
const ISO_REFERENCE: f32 = 100.0;

/// Brightness contribution of an exposure time, in stops.
pub fn shutter_stops(seconds: f32) -> f32 {
    seconds.log2()
}

/// Brightness contribution of an f-number, in stops.
///
/// Two stops per doubling of the f-number, because it is a ratio of diameters
/// and light scales with area.
pub fn aperture_stops(f_number: f32) -> f32 {
    -2.0 * f_number.log2()
}

/// Brightness contribution of a sensitivity, in stops.
pub fn iso_stops(iso: f32) -> f32 {
    (iso / ISO_REFERENCE).log2()
}

/// Parse an exposure time token into seconds.
///
/// Handles the three notations that turn up in practice: fractional
/// (`1/125`), decimal (`0.3`, `30`), and Canon's on-camera form where the
/// double-quote acts as the decimal point (`0"3` is 0.3 s, `30"` is 30 s).
///
/// Returns `None` for `bulb` and `auto`: they have no fixed duration, so a ramp
/// must never treat them as a candidate.
pub fn parse_shutter_seconds(raw: &str) -> Option<f32> {
    let token = raw.trim();
    if token.is_empty()
        || token.eq_ignore_ascii_case("bulb")
        || token.eq_ignore_ascii_case("auto")
    {
        return None;
    }

    if let Some((whole, fraction)) = token.split_once('"') {
        let whole = whole.trim();
        let fraction = fraction.trim();
        return if fraction.is_empty() {
            whole.parse().ok()
        } else {
            format!("{whole}.{fraction}").parse().ok()
        };
    }

    if let Some((numerator, denominator)) = token.split_once('/') {
        let numerator: f32 = numerator.trim().parse().ok()?;
        let denominator: f32 = denominator.trim().parse().ok()?;
        if denominator == 0.0 {
            return None;
        }
        return Some(numerator / denominator);
    }

    token.parse().ok()
}

/// Parse an aperture token (`f4.0`, `f/2.8`, `5.6`) into an f-number.
pub fn parse_f_number(raw: &str) -> Option<f32> {
    let token = raw
        .trim()
        .trim_start_matches(['f', 'F'])
        .trim_start_matches('/')
        .trim();
    if token.is_empty() || token.eq_ignore_ascii_case("auto") {
        return None;
    }
    token.parse().ok().filter(|n: &f32| *n > 0.0)
}

/// Parse a sensitivity token into an ISO number. `auto` yields `None`.
pub fn parse_iso(raw: &str) -> Option<f32> {
    let token = raw.trim().trim_start_matches("ISO").trim();
    if token.is_empty() || token.eq_ignore_ascii_case("auto") {
        return None;
    }
    token.parse().ok().filter(|n: &f32| *n > 0.0)
}

impl Dial {
    /// Brightness contribution of a raw value on this dial, in stops.
    pub fn stops_for(self, raw: &str) -> Option<f32> {
        match self {
            Dial::Shutter => parse_shutter_seconds(raw).map(shutter_stops),
            Dial::Aperture => parse_f_number(raw).map(aperture_stops),
            Dial::Iso => parse_iso(raw).map(iso_stops),
        }
    }
}

impl ExposureValue {
    /// Build a value from the token the camera reported, deriving its stop
    /// position. `raw` is preserved verbatim so it can be echoed back on write.
    pub fn from_raw(dial: Dial, raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let stops = dial.stops_for(&raw);
        let label = pretty_label(dial, &raw);
        Self { raw, label, stops }
    }
}

/// Turn a wire token into something worth putting on screen.
fn pretty_label(dial: Dial, raw: &str) -> String {
    match dial {
        Dial::Shutter => parse_shutter_seconds(raw)
            .map(shutter_label)
            .unwrap_or_else(|| raw.to_uppercase()),
        Dial::Aperture => parse_f_number(raw)
            .map(aperture_label)
            .unwrap_or_else(|| raw.to_string()),
        Dial::Iso => parse_iso(raw)
            .map(iso_label)
            .unwrap_or_else(|| raw.to_uppercase()),
    }
}

/// The standard third-stop shutter series, with the labels cameras print for it.
///
/// This is a table rather than a formula on purpose: the conventional speeds are
/// not clean powers of two (1/8000 is not 2^-13, and the sequence switches from
/// fractions to decimals at 0.4 s by convention, not by arithmetic). Deriving
/// them would produce labels no camera displays.
///
/// Its real job is undoing lossy transport encodings. PTP-IP carries exposure
/// time as a count of 100 µs units, so 1/1600 s arrives as 6 units - which is
/// literally 1/1667. Snapping back onto this series recovers both the label the
/// camera shows and a stop value closer to the physical truth.
#[rustfmt::skip]
const NOMINAL_SHUTTER: &[(f32, &str)] = &[
    (1.0 / 8000.0, "1/8000"), (1.0 / 6400.0, "1/6400"), (1.0 / 5000.0, "1/5000"),
    (1.0 / 4000.0, "1/4000"), (1.0 / 3200.0, "1/3200"), (1.0 / 2500.0, "1/2500"),
    (1.0 / 2000.0, "1/2000"), (1.0 / 1600.0, "1/1600"), (1.0 / 1250.0, "1/1250"),
    (1.0 / 1000.0, "1/1000"), (1.0 / 800.0,  "1/800"),  (1.0 / 640.0,  "1/640"),
    (1.0 / 500.0,  "1/500"),  (1.0 / 400.0,  "1/400"),  (1.0 / 320.0,  "1/320"),
    (1.0 / 250.0,  "1/250"),  (1.0 / 200.0,  "1/200"),  (1.0 / 160.0,  "1/160"),
    (1.0 / 125.0,  "1/125"),  (1.0 / 100.0,  "1/100"),  (1.0 / 80.0,   "1/80"),
    (1.0 / 60.0,   "1/60"),   (1.0 / 50.0,   "1/50"),   (1.0 / 40.0,   "1/40"),
    (1.0 / 30.0,   "1/30"),   (1.0 / 25.0,   "1/25"),   (1.0 / 20.0,   "1/20"),
    (1.0 / 15.0,   "1/15"),   (1.0 / 13.0,   "1/13"),   (1.0 / 10.0,   "1/10"),
    (1.0 / 8.0,    "1/8"),    (1.0 / 6.0,    "1/6"),    (1.0 / 5.0,    "1/5"),
    (1.0 / 4.0,    "1/4"),    (1.0 / 3.0,    "1/3"),
    (0.4, "0.4s"), (0.5, "0.5s"), (0.6, "0.6s"), (0.8, "0.8s"),
    (1.0, "1s"), (1.3, "1.3s"), (1.6, "1.6s"), (2.0, "2s"), (2.5, "2.5s"),
    (3.0, "3s"), (4.0, "4s"), (5.0, "5s"), (6.0, "6s"), (8.0, "8s"),
    (10.0, "10s"), (13.0, "13s"), (15.0, "15s"), (20.0, "20s"), (25.0, "25s"),
    (30.0, "30s"),
];

/// How far off a value may be and still count as one of the standard speeds.
///
/// Slightly wider than one third-stop step, because the coarsest encoding error
/// we have to absorb is 1/8000 arriving as 1/10000 - a third of a stop. Since we
/// always take the *nearest* entry and camera value lists are the standard
/// series, a generous window cannot pick a wrong neighbour.
const SHUTTER_SNAP_TOLERANCE_STOPS: f32 = 0.4;

/// Find the standard shutter speed a value was meant to be.
///
/// Returns the exact seconds and the camera's own label. `None` when the value is
/// too far from any standard speed to be one of them.
pub fn snap_shutter(seconds: f32) -> Option<(f32, &'static str)> {
    if seconds <= 0.0 {
        return None;
    }
    let target = seconds.log2();

    let (nominal, label, distance) = NOMINAL_SHUTTER.iter().fold(
        (0.0f32, "", f32::INFINITY),
        |best, (candidate, label)| {
            let distance = (candidate.log2() - target).abs();
            if distance < best.2 {
                (*candidate, *label, distance)
            } else {
                best
            }
        },
    );

    (distance <= SHUTTER_SNAP_TOLERANCE_STOPS).then_some((nominal, label))
}

/// Exposure time as a photographer reads it off a camera back.
///
/// Public because backends that receive numeric values rather than strings -
/// PTP-IP encodes exposure time as an integer count - need the same formatting.
pub fn shutter_label(seconds: f32) -> String {
    if let Some((_, label)) = snap_shutter(seconds) {
        return label.to_string();
    }
    // Outside the standard series: describe it rather than pretend.
    match seconds {
        s if s < 0.5 => format!("1/{}", (1.0 / s).round() as i64),
        s if s.fract() == 0.0 => format!("{}s", s as i64),
        s => format!("{s:.1}s"),
    }
}

pub fn aperture_label(f_number: f32) -> String {
    format!("f/{f_number}")
}

pub fn iso_label(iso: f32) -> String {
    format!("{}", iso as i64)
}

impl ExposureSettings {
    /// Total brightness of the current settings, in stops.
    ///
    /// `None` if any dial is on a value without a fixed brightness - the ramp
    /// has no reference point in that case and should say so rather than guess.
    pub fn total_stops(&self) -> Option<f32> {
        let shutter = self.shutter.as_ref()?.stops?;
        let aperture = self.aperture.as_ref()?.stops?;
        let iso = self.iso.as_ref()?.stops?;
        Some(shutter + aperture + iso)
    }
}

/// Pick the selectable value closest to a target brightness.
///
/// Values without a stop position (`bulb`, `auto`) are skipped. Ties go to the
/// earlier entry, which keeps the choice stable across repeated calls - a ramp
/// that oscillates between two equidistant values produces visible flicker.
pub fn nearest(values: &[ExposureValue], target_stops: f32) -> Option<&ExposureValue> {
    values
        .iter()
        .filter_map(|value| value.stops.map(|stops| (value, stops)))
        .fold(None, |best, (value, stops)| {
            let distance = (stops - target_stops).abs();
            match best {
                Some((_, best_distance)) if best_distance <= distance => best,
                _ => Some((value, distance)),
            }
        })
        .map(|(value, _)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-4, "{a} != {b}");
    }

    #[test]
    fn parses_the_notations_cameras_actually_send() {
        approx(parse_shutter_seconds("1/125").unwrap(), 0.008);
        approx(parse_shutter_seconds("30").unwrap(), 30.0);
        approx(parse_shutter_seconds("0.3").unwrap(), 0.3);
        // Canon's on-camera notation.
        approx(parse_shutter_seconds("0\"3").unwrap(), 0.3);
        approx(parse_shutter_seconds("30\"").unwrap(), 30.0);
        assert!(parse_shutter_seconds("bulb").is_none());
        assert!(parse_shutter_seconds("1/0").is_none());

        approx(parse_f_number("f4.0").unwrap(), 4.0);
        approx(parse_f_number("f/2.8").unwrap(), 2.8);
        approx(parse_f_number("5.6").unwrap(), 5.6);
        assert!(parse_f_number("auto").is_none());

        approx(parse_iso("400").unwrap(), 400.0);
        assert!(parse_iso("auto").is_none());
    }

    #[test]
    fn stops_are_anchored_where_we_claim() {
        // One second, f/1.0, ISO 100 is the zero point of the scale.
        approx(shutter_stops(1.0), 0.0);
        approx(aperture_stops(1.0), 0.0);
        approx(iso_stops(100.0), 0.0);

        // Doubling exposure time is one stop brighter.
        approx(shutter_stops(2.0) - shutter_stops(1.0), 1.0);
        // Doubling the f-number is two stops darker.
        approx(aperture_stops(2.8) - aperture_stops(5.6), 2.0);
        // Doubling ISO is one stop brighter.
        approx(iso_stops(400.0) - iso_stops(200.0), 1.0);
    }

    #[test]
    fn total_brightness_is_the_sum_of_the_dials() {
        let settings = ExposureSettings {
            shutter: Some(ExposureValue::from_raw(Dial::Shutter, "1/125")),
            aperture: Some(ExposureValue::from_raw(Dial::Aperture, "f4.0")),
            iso: Some(ExposureValue::from_raw(Dial::Iso, "400")),
        };
        // log2(1/125) - 2*log2(4) + log2(4) = -6.9658 - 4 + 2
        approx(settings.total_stops().unwrap(), -8.9658);
    }

    #[test]
    fn bulb_defeats_total_brightness_instead_of_guessing() {
        let settings = ExposureSettings {
            shutter: Some(ExposureValue::from_raw(Dial::Shutter, "bulb")),
            aperture: Some(ExposureValue::from_raw(Dial::Aperture, "f4.0")),
            iso: Some(ExposureValue::from_raw(Dial::Iso, "100")),
        };
        assert!(settings.total_stops().is_none());
    }

    #[test]
    fn nearest_snaps_onto_a_selectable_value() {
        let shutter: Vec<_> = ["1/125", "1/60", "1/30", "bulb"]
            .iter()
            .map(|raw| ExposureValue::from_raw(Dial::Shutter, *raw))
            .collect();

        // Ask for exactly 1/60 and get it.
        let target = shutter_stops(1.0 / 60.0);
        assert_eq!(nearest(&shutter, target).unwrap().raw, "1/60");

        // Ask for something between 1/60 and 1/30 and get the closer one.
        let target = shutter_stops(1.0 / 40.0);
        assert_eq!(nearest(&shutter, target).unwrap().raw, "1/30");

        // Far outside the range: clamp to the end, never to `bulb`.
        assert_eq!(nearest(&shutter, 10.0).unwrap().raw, "1/30");
        assert_eq!(nearest(&shutter, -20.0).unwrap().raw, "1/125");
    }

    #[test]
    fn nearest_ignores_values_without_a_stop_position() {
        let iso: Vec<_> = ["auto"]
            .iter()
            .map(|raw| ExposureValue::from_raw(Dial::Iso, *raw))
            .collect();
        assert!(nearest(&iso, 0.0).is_none());
    }

    #[test]
    fn labels_read_like_a_camera_display() {
        assert_eq!(ExposureValue::from_raw(Dial::Shutter, "0\"3").label, "1/3");
        assert_eq!(ExposureValue::from_raw(Dial::Shutter, "30\"").label, "30s");
        assert_eq!(ExposureValue::from_raw(Dial::Shutter, "bulb").label, "BULB");
        assert_eq!(ExposureValue::from_raw(Dial::Aperture, "f4.0").label, "f/4");
        assert_eq!(ExposureValue::from_raw(Dial::Iso, "auto").label, "AUTO");
    }
}
