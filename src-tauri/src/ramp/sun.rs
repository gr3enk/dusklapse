//! Where the sun is, and how bright the sky ought to be because of it.
//!
//! Computed locally from a position and a timestamp. No network: the app is meant to run on a
//! camera's own Wi-Fi with no route to the internet, so anything that needed a service would
//! stop working exactly when it is used.
//!
//! # Accuracy
//!
//! The low-precision solar position from the Astronomical Almanac, good to about a hundredth of
//! a degree over a few centuries either side of 2000. Twilight boundaries are defined in whole
//! degrees, so that is three orders of magnitude more than this needs - the limiting factor here
//! is how well the device knows where it is, not the ephemeris.

use serde::{Deserialize, Serialize};

/// Where the camera is, in degrees. WGS84, as every GPS reports.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

/// What the sky is doing, in the vocabulary photographers use for it.
///
/// The boundaries are the standard ones, except that golden and blue hour are named separately
/// inside what an almanac would simply call civil twilight - because those are the names anyone
/// planning a shoot actually uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkyPhase {
    /// Sun well up.
    Day,
    /// Sun low but still above the horizon.
    GoldenHour,
    /// Sun just below the horizon. Civil twilight.
    BlueHour,
    NauticalTwilight,
    AstronomicalTwilight,
    /// Sun more than 18° down: as dark as it gets.
    Night,
}

/// Elevation at which the sun's upper limb touches the horizon, allowing for refraction and the
/// sun's own radius. The conventional value for sunrise and sunset.
const HORIZON_DEGREES: f64 = -0.833;
const GOLDEN_HOUR_TOP_DEGREES: f64 = 6.0;
const CIVIL_DEGREES: f64 = -6.0;
const NAUTICAL_DEGREES: f64 = -12.0;
/// Below this the sky no longer gets measurably darker, so it is the floor of the curve.
const ASTRONOMICAL_DEGREES: f64 = -18.0;

impl SkyPhase {
    pub fn of(elevation_degrees: f64) -> Self {
        match elevation_degrees {
            e if e > GOLDEN_HOUR_TOP_DEGREES => SkyPhase::Day,
            e if e > HORIZON_DEGREES => SkyPhase::GoldenHour,
            e if e > CIVIL_DEGREES => SkyPhase::BlueHour,
            e if e > NAUTICAL_DEGREES => SkyPhase::NauticalTwilight,
            e if e > ASTRONOMICAL_DEGREES => SkyPhase::AstronomicalTwilight,
            _ => SkyPhase::Night,
        }
    }
}

/// Sun elevation above the horizon, in degrees, at a place and an instant.
///
/// `unix_seconds` is plain seconds since the epoch - the same thing `SystemTime` gives - so no
/// calendar library is involved. The Julian day comes straight out of it.
pub fn elevation(location: Location, unix_seconds: f64) -> f64 {
    // Days since J2000.0. The epoch is 2451545.0 in Julian days, and the Unix epoch is
    // 2440587.5.
    let n = unix_seconds / 86_400.0 + 2_440_587.5 - 2_451_545.0;

    // Mean longitude and mean anomaly of the sun.
    let mean_longitude = (280.460 + 0.985_647_4 * n).rem_euclid(360.0);
    let mean_anomaly = (357.528 + 0.985_600_3 * n).rem_euclid(360.0).to_radians();

    // Ecliptic longitude: the mean longitude corrected for the orbit not being circular.
    let ecliptic = (mean_longitude + 1.915 * mean_anomaly.sin() + 0.020 * (2.0 * mean_anomaly).sin())
        .to_radians();

    // Tilt of the earth's axis, drifting very slowly.
    let obliquity = (23.439 - 0.000_000_4 * n).to_radians();

    // Equatorial coordinates of the sun.
    let right_ascension = (obliquity.cos() * ecliptic.sin()).atan2(ecliptic.cos());
    let declination = (obliquity.sin() * ecliptic.sin()).asin();

    // Greenwich mean sidereal time, then local: how far the place has rotated past the stars.
    let gmst_hours = (18.697_374_558 + 24.065_709_824_419_08 * n).rem_euclid(24.0);
    let local_sidereal = (gmst_hours * 15.0 + location.longitude).to_radians();

    // How far the sun is from the local meridian.
    let hour_angle = local_sidereal - right_ascension;

    let latitude = location.latitude.to_radians();
    let sin_elevation = latitude.sin() * declination.sin()
        + latitude.cos() * declination.cos() * hour_angle.cos();

    sin_elevation.clamp(-1.0, 1.0).asin().to_degrees()
}

/// How much daylight there is, from 1.0 in the day to 0.0 at astronomical night.
///
/// Linear in solar elevation between the horizon and 18° below it. Deliberately a *stylistic*
/// curve rather than a model of sky luminance - real sky brightness falls far faster than this
/// and by ten stops or more, which no timelapse is graded to. What this drives is how much
/// darker the finished sequence should look at night, which is a look, not a measurement.
pub fn daylight_fraction(elevation_degrees: f64) -> f32 {
    let span = HORIZON_DEGREES - ASTRONOMICAL_DEGREES;
    let above_floor = elevation_degrees - ASTRONOMICAL_DEGREES;
    (above_floor / span).clamp(0.0, 1.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Seconds since the Unix epoch for a UTC calendar date and time.
    ///
    /// Written out rather than pulled from a date library so the tests state their own inputs:
    /// days from 1970 to the year, plus days into the year.
    fn utc(year: i64, month: u32, day: u32, hour: f64, minute: f64) -> f64 {
        let mut days: i64 = 0;
        for y in 1970..year {
            days += if leap(y) { 366 } else { 365 };
        }
        let lengths = [31, if leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        days += lengths[..month as usize - 1].iter().sum::<i64>();
        days += day as i64 - 1;
        days as f64 * 86_400.0 + hour * 3_600.0 + minute * 60.0
    }

    fn leap(year: i64) -> bool {
        (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
    }

    const BERLIN: Location = Location {
        latitude: 52.52,
        longitude: 13.40,
    };

    /// Solar noon elevation at the equinox equals 90° minus the latitude. Geometry, independent
    /// of any implementation - which is the point of testing against it rather than against
    /// numbers from another program.
    #[test]
    fn equinox_noon_elevation_is_ninety_minus_latitude() {
        // At the March equinox the sun stands over the equator, so at local solar noon on the
        // Greenwich meridian its elevation is 90° minus the latitude.
        for (latitude, expected) in [(0.0, 90.0), (52.52, 37.48), (-33.87, 56.13)] {
            let location = Location { latitude, longitude: 0.0 };
            let elevation = elevation(location, utc(2026, 3, 20, 12.0, 7.0));
            assert!(
                (elevation - expected).abs() < 0.6,
                "at {latitude}° got {elevation}, expected about {expected}"
            );
        }
    }

    /// At the solstices the sun is 23.44° north or south of the equator, which shifts noon by
    /// exactly that much either way.
    #[test]
    fn the_solstices_shift_noon_by_the_axial_tilt() {
        let tilt = 23.44;
        let midsummer = elevation(BERLIN, utc(2026, 6, 21, 11.0, 6.0));
        let midwinter = elevation(BERLIN, utc(2026, 12, 21, 11.0, 6.0));

        assert!(
            (midsummer - (90.0 - BERLIN.latitude + tilt)).abs() < 0.8,
            "midsummer noon {midsummer}"
        );
        assert!(
            (midwinter - (90.0 - BERLIN.latitude - tilt)).abs() < 0.8,
            "midwinter noon {midwinter}"
        );
    }

    /// Midnight has to put the sun below the horizon, and by roughly the mirror of noon.
    #[test]
    fn midnight_puts_the_sun_below_the_horizon() {
        let midnight = elevation(BERLIN, utc(2026, 12, 21, 23.0, 6.0));
        assert!(midnight < -50.0, "winter midnight {midnight}");
    }

    /// Berlin in high summer never reaches astronomical night - the sun stays above -18° all
    /// night, which is why there is no truly dark sky there in June.
    #[test]
    fn a_northern_summer_night_never_gets_fully_dark() {
        let mut lowest = f64::MAX;
        for minute in (0..1440).step_by(10) {
            let e = elevation(BERLIN, utc(2026, 6, 21, 0.0, minute as f64));
            lowest = lowest.min(e);
        }
        assert!(lowest > ASTRONOMICAL_DEGREES, "lowest elevation {lowest}");
        assert_ne!(SkyPhase::of(lowest), SkyPhase::Night);
    }

    /// Sunset in Berlin on this date is a few minutes past 21:00 local, which is 19:0x UTC in
    /// summer time. The crossing has to land there.
    #[test]
    fn the_horizon_crossing_lands_at_the_published_sunset() {
        let before = elevation(BERLIN, utc(2026, 7, 28, 18.0, 45.0));
        let after = elevation(BERLIN, utc(2026, 7, 28, 19.0, 30.0));

        assert!(before > HORIZON_DEGREES, "still up at 18:45 UTC, got {before}");
        assert!(after < HORIZON_DEGREES, "already down at 19:30 UTC, got {after}");
    }

    /// The southern hemisphere runs the other way, which a hardcoded northern assumption would
    /// get backwards.
    #[test]
    fn the_southern_hemisphere_has_its_seasons_reversed() {
        let sydney = Location { latitude: -33.87, longitude: 151.21 };
        // Local noon in Sydney is around 02:00 UTC.
        let december = elevation(sydney, utc(2026, 12, 21, 1.0, 0.0));
        let june = elevation(sydney, utc(2026, 6, 21, 2.0, 0.0));
        assert!(december > june, "December {december} should beat June {june} in Sydney");
    }

    #[test]
    fn phases_follow_the_standard_boundaries() {
        assert_eq!(SkyPhase::of(30.0), SkyPhase::Day);
        assert_eq!(SkyPhase::of(3.0), SkyPhase::GoldenHour);
        assert_eq!(SkyPhase::of(-3.0), SkyPhase::BlueHour);
        assert_eq!(SkyPhase::of(-9.0), SkyPhase::NauticalTwilight);
        assert_eq!(SkyPhase::of(-15.0), SkyPhase::AstronomicalTwilight);
        assert_eq!(SkyPhase::of(-40.0), SkyPhase::Night);
    }

    #[test]
    fn daylight_runs_from_one_to_zero_across_twilight() {
        assert_eq!(daylight_fraction(45.0), 1.0);
        assert_eq!(daylight_fraction(HORIZON_DEGREES), 1.0);
        assert_eq!(daylight_fraction(ASTRONOMICAL_DEGREES), 0.0);
        assert_eq!(daylight_fraction(-60.0), 0.0);

        // Halfway down twilight is halfway through the curve.
        let middle = daylight_fraction((HORIZON_DEGREES + ASTRONOMICAL_DEGREES) / 2.0);
        assert!((middle - 0.5).abs() < 1e-4, "{middle}");
    }

    /// The curve must never leave 0..1, whatever elevation it is handed.
    #[test]
    fn daylight_is_always_a_fraction() {
        for elevation in [-90.0, -18.1, -18.0, -9.0, 0.0, 0.5, 90.0] {
            let fraction = daylight_fraction(elevation);
            assert!((0.0..=1.0).contains(&fraction), "{elevation} gave {fraction}");
        }
    }
}
