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
        Dial::Shutter => match parse_shutter_seconds(raw) {
            // Sub-second speeds read best as the fraction the photographer
            // knows, whatever notation the camera happened to use.
            Some(seconds) if seconds < 0.5 => format!("1/{}", (1.0 / seconds).round() as i64),
            Some(seconds) if seconds < 1.0 => format!("{seconds:.1}s"),
            Some(seconds) if seconds.fract() == 0.0 => format!("{}s", seconds as i64),
            Some(seconds) => format!("{seconds:.1}s"),
            None => raw.to_uppercase(),
        },
        Dial::Aperture => match parse_f_number(raw) {
            Some(f) => format!("f/{f}"),
            None => raw.to_string(),
        },
        Dial::Iso => match parse_iso(raw) {
            Some(iso) => format!("{}", iso as i64),
            None => raw.to_uppercase(),
        },
    }
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
