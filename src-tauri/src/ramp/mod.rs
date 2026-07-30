//! Holy-grail ramp configuration.
//!
//! # Why this lives in Rust and not in React state
//!
//! The same reason the camera session does. Three of them, in order of how much they
//! hurt:
//!
//! 1. **A WebView reload must not lose it.** That happens constantly in development and
//!    can happen in the field; losing the reference mid-sequence would silently stop the
//!    ramp from correcting.
//! 2. **The engine that consumes it will run here.** It has to react to frame events with
//!    tight timing and keep working while the WebView is idle or backgrounded, which rules
//!    out the settings living on the other side of an IPC boundary.
//! 3. **The arithmetic is already here.** [`Luminance::stops_from`] turns a reference and a
//!    measurement into the correction in stops. Keeping the reference next to it means
//!    there is one definition of what "how far off are we" means.
//!
//! The frontend holds a rendering copy and writes through. Every mutating command returns
//! the stored settings, so the UI never has to guess what was actually kept.

pub mod plan;
pub mod sun;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::camera::{Dial, Luminance};
use sun::{Location, SkyPhase};

/// Which way the light is going.
///
/// Not cosmetic: it is the sign of the expected drift, and a ramp that assumes the wrong
/// direction fights the sky instead of following it. At sunset the scene darkens, so
/// exposure has to open up over time; at sunrise the opposite. It also decides what to do
/// with a reading that is off by an implausible amount - a passing car at dusk is noise,
/// the same jump at dawn may be the actual sunrise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RampMode {
    Sunset,
    Sunrise,
}

/// How far the ramp may take one dial, and whether it may touch it at all.
///
/// One limit per dial rather than two, because there is only ever one: the ramp travels in
/// the direction the light is going, and the limit is the far end of that travel. At sunset
/// the scene darkens and exposure opens up, so the limit is the longest shutter, the highest
/// ISO, the widest aperture; at sunrise everything runs the other way and the same field is
/// the shortest, lowest, smallest. The two readings are the same number wearing a different
/// name, so naming them is the UI's job and storing them twice would let them disagree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialRamp {
    /// Whether the ramp may move this dial.
    pub enabled: bool,
    /// The far end of the travel, as a token the camera itself reported.
    ///
    /// A raw token rather than a number, for the same reason every other value in this
    /// codebase is: it is the camera's own vocabulary, and synthesising one produces a value
    /// the body rejects. `None` until one is chosen.
    ///
    /// Deliberately not validated on write. Which values a dial offers changes with the
    /// shooting mode and the attached lens, so a limit that is legal now can stop being
    /// offered later - the engine resolves it against the current list and snaps to the
    /// nearest, which handles both cases without a stored value going stale.
    pub limit: Option<String>,
}

/// How the darkening is spread across the event.
///
/// All three start and end in the same place - full daylight leaves the reference alone, full
/// night applies the whole factor. They differ only in where the change is concentrated, which is
/// what decides whether a sequence holds its brightness into dusk and then drops, or gives most of
/// it up early and coasts.
///
/// Named for how they run **in time**, and they mean the same thing in both modes: the shape is
/// applied to the progress through the event, which counts from full day at sunset and from full
/// night at sunrise. A sunrise therefore gives back what a sunset accumulated, in the same order.
/// That is why the icons for the two directions are crossed over in the UI rather than mirrored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DaylightShape {
    /// Proportional to the progress through the event.
    Linear,
    /// Little at first, most of it towards the end.
    SlowThenFast,
    /// Most of it at the start, tapering off.
    FastThenSlow,
}

impl DaylightShape {
    /// Map progress through the event onto how much of the factor has been applied.
    ///
    /// Both curves are quadratic: steep enough to be visible in a finished sequence, gentle enough
    /// that the ramp is not asked for a sudden run of corrections. Every one of them satisfies
    /// `f(0) = 0` and `f(1) = 1`, which is what keeps the endpoints identical across all three.
    fn apply(self, progress: f32) -> f32 {
        let t = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::SlowThenFast => t * t,
            Self::FastThenSlow => 1.0 - (1.0 - t) * (1.0 - t),
        }
    }
}

/// Darkens the reference as the sky darkens, so the finished sequence does too.
///
/// Without it, a sunset ramp holds one brightness all the way into the night and the result
/// looks like an evenly lit day that happens to contain stars. What people actually want is for
/// night to *read* as night - darker than dusk, but not so dark it is unusable.
///
/// Optional on purpose. Over a lit city the sky stops being what sets the exposure long before
/// astronomical night, and forcing the reference down there would just underexpose the lights.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DaylightCurve {
    pub enabled: bool,
    /// How much darker night should look than day, as a brightness ratio.
    ///
    /// 2.0 means half as bright, which is one stop. 4.0 is two stops. Never below 1.0 - that
    /// would brighten the sequence as it got dark.
    pub factor: f32,
    /// Where the camera is. `None` until a position is supplied, which disables the curve
    /// however it is configured: there is no sun elevation without a place to stand.
    pub location: Option<Location>,
    /// How the darkening is distributed across the event.
    pub shape: DaylightShape,
}

impl Default for DaylightCurve {
    fn default() -> Self {
        Self {
            enabled: false,
            // One stop between day and night. Enough to read as night without losing the scene.
            factor: 2.0,
            location: None,
            // The behaviour this feature had before the shapes existed.
            shape: DaylightShape::Linear,
        }
    }
}

/// What the sky is doing right now, and what that does to the reference.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkyState {
    pub elevation_degrees: f32,
    pub phase: SkyPhase,
    /// 1.0 in daylight, 0.0 at astronomical night.
    pub daylight: f32,
    /// The reference the ramp is actually aiming at, after the curve.
    pub effective_reference: Luminance,
    /// Stops the curve has taken off the stored reference. Zero or negative.
    pub offset_stops: f32,
}

/// What the ramp is aiming for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RampSettings {
    /// Whether the ramp is allowed to move the camera at all.
    ///
    /// Separate from having a reference on purpose: you set a reference by looking at a
    /// good frame, and that has to be possible before arming anything.
    pub active: bool,
    pub mode: RampMode,
    /// The brightness the ramp holds the sequence at.
    pub reference: Luminance,
    pub shutter: DialRamp,
    pub aperture: DialRamp,
    pub iso: DialRamp,
    pub daylight: DaylightCurve,
}

impl Default for RampSettings {
    fn default() -> Self {
        Self {
            active: false,
            mode: RampMode::Sunset,
            // Mid-grey. A sane starting point that is never right for a real composition -
            // the reference is something you set from a frame you like, and half scale is
            // the least misleading placeholder until you do.
            reference: Luminance::from_value(5000),

            // Shutter and ISO on, aperture off. Not an arbitrary split: a mechanical
            // aperture steps in coarse increments and never lands in exactly the same place
            // twice, so ramping it produces visible flicker between frames and a shifting
            // depth of field. It stays available for anyone who wants it and off by default
            // because the usual answer is to leave it alone.
            shutter: DialRamp {
                enabled: true,
                limit: None,
            },
            aperture: DialRamp {
                enabled: false,
                limit: None,
            },
            iso: DialRamp {
                enabled: true,
                limit: None,
            },
            daylight: DaylightCurve::default(),
        }
    }
}

impl RampSettings {
    /// How much daylight there is where the camera stands, or `None` when the curve is off or
    /// has no position to work from.
    pub fn daylight_now(&self, unix_seconds: f64) -> Option<(f64, f32)> {
        let location = self.daylight.location?;
        if !self.daylight.enabled {
            return None;
        }
        let elevation = sun::elevation(location, unix_seconds);
        Some((elevation, sun::daylight_fraction(elevation)))
    }

    /// Stops the curve takes off the stored reference at a given amount of daylight.
    ///
    /// Interpolated in stops rather than in the reported value, because stops are what exposure
    /// and perception are both linear in - the same factor then produces the same *look* wherever
    /// the reference happens to sit on the scale.
    ///
    /// The mode is consulted because the shape describes progress through the *event*, not the
    /// amount of daylight: a sunset counts from full day, a sunrise from full night. Without that,
    /// a shape chosen at sunset would come out mirrored at sunrise. `Linear` is unaffected either
    /// way, which is why this reads the same as it did before the shapes existed.
    fn offset_stops(&self, daylight: f32) -> f32 {
        let daylight = daylight.clamp(0.0, 1.0);
        let progress = match self.mode {
            RampMode::Sunset => 1.0 - daylight,
            RampMode::Sunrise => daylight,
        };

        let applied = self.daylight.shape.apply(progress);
        // A sunset accumulates the darkening; a sunrise hands it back.
        let magnitude = match self.mode {
            RampMode::Sunset => applied,
            RampMode::Sunrise => 1.0 - applied,
        };

        -magnitude * self.daylight.factor.max(1.0).log2()
    }

    /// The reference the ramp should actually aim at.
    ///
    /// With no daylight information this is simply the stored reference, so every other part of
    /// the ramp behaves identically whether the curve is on or off.
    pub fn effective_reference(&self, daylight: Option<f32>) -> Luminance {
        match daylight {
            Some(daylight) => Luminance::from_linear(
                self.reference.linear * 2f32.powf(self.offset_stops(daylight)),
            ),
            None => self.reference,
        }
    }

    /// The stored reference that would make *now* look right.
    ///
    /// The inverse of [`Self::effective_reference`], for the "use this frame" button. Without it,
    /// anchoring on a frame during twilight would store the measurement as the day value and the
    /// target would immediately jump darker - the curve would fight the thing that was just
    /// chosen.
    pub fn base_from_measured(&self, measured: Luminance, daylight: Option<f32>) -> Luminance {
        match daylight {
            Some(daylight) => {
                Luminance::from_linear(measured.linear * 2f32.powf(-self.offset_stops(daylight)))
            }
            None => measured,
        }
    }

    /// Everything the UI needs to explain what the curve is doing.
    pub fn sky(&self, unix_seconds: f64) -> Option<SkyState> {
        let (elevation, daylight) = self.daylight_now(unix_seconds)?;
        Some(SkyState {
            elevation_degrees: elevation as f32,
            phase: SkyPhase::of(elevation),
            daylight,
            effective_reference: self.effective_reference(Some(daylight)),
            offset_stops: self.offset_stops(daylight),
        })
    }

    /// The per-dial configuration, addressed the way [`crate::camera::ExposureCapabilities`]
    /// and [`crate::camera::ExposureSettings`] are, so the planner can walk the dials in
    /// order rather than naming each field.
    pub fn dial(&self, dial: Dial) -> &DialRamp {
        match dial {
            Dial::Shutter => &self.shutter,
            Dial::Aperture => &self.aperture,
            Dial::Iso => &self.iso,
        }
    }
}

/// The live ramp configuration.
///
/// A lock rather than a channel because every reader wants the current value, not a
/// history of changes.
#[derive(Default)]
pub struct RampState(RwLock<RampSettings>);

impl RampState {
    pub async fn get(&self) -> RampSettings {
        self.0.read().await.clone()
    }

    /// Store a configuration, rebuilding the reference so it cannot be internally
    /// inconsistent.
    ///
    /// A [`Luminance`] carries the same brightness twice: `value` for display and `linear`
    /// for arithmetic. A caller that edits one and forgets the other - which is exactly
    /// what a spinner bound to `value` does - would store a reference whose displayed
    /// number and whose maths disagree, and the ramp would quietly aim at the wrong
    /// brightness. Recomputing here means the invariant is enforced by the side that owns
    /// the conversion instead of trusted across an IPC boundary.
    pub async fn set(&self, settings: RampSettings) -> RampSettings {
        let mut guard = self.0.write().await;
        *guard = RampSettings {
            reference: Luminance::from_value(settings.reference.value),
            daylight: DaylightCurve {
                // Below 1.0 the curve would brighten the sequence as the sky darkened, which is
                // not a setting anyone wants and is easier to refuse here than to guard against
                // at every use.
                factor: settings.daylight.factor.max(1.0),
                ..settings.daylight
            },
            ..settings
        };
        guard.clone()
    }

    /// Point the reference at a brightness that was just measured.
    ///
    /// Stores the *base* reference, not the measurement: with the daylight curve running, the
    /// target already sits below the base, so storing the measurement directly would make the
    /// ramp immediately want the frame darker than the one just chosen as correct.
    pub async fn set_reference(&self, measured: Luminance, unix_seconds: f64) -> RampSettings {
        let mut guard = self.0.write().await;
        let daylight = guard
            .daylight_now(unix_seconds)
            .map(|(_, daylight)| daylight);
        guard.reference = guard.base_from_measured(measured, daylight);
        guard.clone()
    }
}

/// Seconds since the Unix epoch, as the sun math wants them.
pub fn now_unix_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        // A clock before 1970 is a broken clock; treating it as the epoch keeps the ramp running
        // on the stored reference rather than panicking mid-sequence.
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Berlin, for tests that need a real place to put the sun in.
    const BERLIN: Location = Location {
        latitude: 52.52,
        longitude: 13.40,
    };

    fn with_curve(factor: f32, location: Option<Location>) -> RampSettings {
        shaped(factor, location, DaylightShape::Linear, RampMode::Sunset)
    }

    fn shaped(
        factor: f32,
        location: Option<Location>,
        shape: DaylightShape,
        mode: RampMode,
    ) -> RampSettings {
        RampSettings {
            active: true,
            mode,
            reference: Luminance::from_value(5000),
            daylight: DaylightCurve {
                enabled: true,
                factor,
                location,
                shape,
            },
            ..Default::default()
        }
    }

    /// How much of the factor has been applied, from 0 to 1. The quantity the shapes act on.
    fn applied_fraction(settings: &RampSettings, daylight: f32) -> f32 {
        let target = settings.effective_reference(Some(daylight));
        let stops = target
            .stops_from(settings.reference)
            .expect("both positive");
        -stops / settings.daylight.factor.log2()
    }

    const SHAPES: [DaylightShape; 3] = [
        DaylightShape::Linear,
        DaylightShape::SlowThenFast,
        DaylightShape::FastThenSlow,
    ];

    /// Whatever the shape, the two ends are fixed: full daylight leaves the reference alone and
    /// full night applies the whole factor. A shape may only redistribute what happens in between.
    #[test]
    fn every_shape_shares_the_same_endpoints() {
        for shape in SHAPES {
            for mode in [RampMode::Sunset, RampMode::Sunrise] {
                let settings = shaped(2.0, Some(BERLIN), shape, mode);
                let day = applied_fraction(&settings, 1.0);
                let night = applied_fraction(&settings, 0.0);
                assert!(
                    (day - 0.0).abs() < 1e-4,
                    "{shape:?} {mode:?} applied {day} in full daylight"
                );
                assert!(
                    (night - 1.0).abs() < 1e-4,
                    "{shape:?} {mode:?} applied {night} at night"
                );
            }
        }
    }

    /// Halfway through a sunset the three shapes have to be in a definite order, or the setting
    /// makes no visible difference.
    #[test]
    fn the_shapes_are_ordered_at_the_midpoint_of_a_sunset() {
        let half =
            |shape| applied_fraction(&shaped(2.0, Some(BERLIN), shape, RampMode::Sunset), 0.5);

        let slow = half(DaylightShape::SlowThenFast);
        let linear = half(DaylightShape::Linear);
        let fast = half(DaylightShape::FastThenSlow);

        assert!(
            (linear - 0.5).abs() < 1e-4,
            "linear should be exactly half way, got {linear}"
        );
        assert!(
            slow < linear,
            "slow-then-fast should lag behind linear: {slow} vs {linear}"
        );
        assert!(
            fast > linear,
            "fast-then-slow should lead linear: {fast} vs {linear}"
        );
        // Quadratics, so the two sit symmetrically either side of the middle.
        assert!(((linear - slow) - (fast - linear)).abs() < 1e-4);
    }

    /// `Linear` must behave exactly as it did before shapes existed - in both directions.
    #[test]
    fn linear_is_unaffected_by_the_mode() {
        for daylight in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let sunset = applied_fraction(
                &shaped(2.0, Some(BERLIN), DaylightShape::Linear, RampMode::Sunset),
                daylight,
            );
            let sunrise = applied_fraction(
                &shaped(2.0, Some(BERLIN), DaylightShape::Linear, RampMode::Sunrise),
                daylight,
            );
            assert!(
                (sunset - sunrise).abs() < 1e-4,
                "at {daylight} daylight: {sunset} vs {sunrise}"
            );
            assert!((sunset - (1.0 - daylight)).abs() < 1e-4);
        }
    }

    /// A sunrise gives back what a sunset accumulated, in the same order.
    ///
    /// This is the invariant the crossed-over icons in the UI depend on: at the same progress
    /// through the event, the shape looks the same in both directions, so a sunrise reads as the
    /// mirror image of a sunset rather than as a different curve.
    #[test]
    fn a_sunrise_undoes_a_sunset_in_the_same_order() {
        for shape in SHAPES {
            let sunset = shaped(3.0, Some(BERLIN), shape, RampMode::Sunset);
            let sunrise = shaped(3.0, Some(BERLIN), shape, RampMode::Sunrise);

            for step in 0..=10 {
                let progress = step as f32 / 10.0;
                // Progress runs from full day at sunset and from full night at sunrise.
                let during_sunset = applied_fraction(&sunset, 1.0 - progress);
                let during_sunrise = applied_fraction(&sunrise, progress);
                assert!(
                    (during_sunset + during_sunrise - 1.0).abs() < 1e-4,
                    "{shape:?} at progress {progress}: {during_sunset} + {during_sunrise} should be 1"
                );
            }
        }
    }

    /// No shape may make the target wander back up as the light keeps falling.
    #[test]
    fn every_shape_falls_monotonically_through_a_sunset() {
        for shape in SHAPES {
            let settings = shaped(3.0, Some(BERLIN), shape, RampMode::Sunset);
            let mut previous = f32::MAX;
            for step in 0..=20 {
                let daylight = 1.0 - step as f32 / 20.0;
                let target = settings.effective_reference(Some(daylight)).linear;
                assert!(target <= previous, "{shape:?} rose at {daylight} daylight");
                previous = target;
            }
        }
    }

    /// Anchoring on a frame has to round-trip for every shape, not just the linear one.
    #[test]
    fn anchoring_round_trips_for_every_shape() {
        let measured = Luminance::from_value(2269);
        for shape in SHAPES {
            for mode in [RampMode::Sunset, RampMode::Sunrise] {
                let settings = shaped(2.0, Some(BERLIN), shape, mode);
                for daylight in [0.0, 0.3, 0.75, 1.0] {
                    let base = settings.base_from_measured(measured, Some(daylight));
                    let held = RampSettings {
                        reference: base,
                        ..settings.clone()
                    }
                    .effective_reference(Some(daylight));
                    assert!(
                        (held.linear - measured.linear).abs() < 1e-6,
                        "{shape:?} {mode:?} at {daylight}: came back as {} not {}",
                        held.value,
                        measured.value
                    );
                }
            }
        }
    }

    /// Switched off, the curve must be invisible - every other part of the ramp has to behave
    /// exactly as it did before the feature existed.
    #[test]
    fn a_disabled_curve_leaves_the_reference_alone() {
        let settings = RampSettings {
            reference: Luminance::from_value(4200),
            ..Default::default()
        };
        assert!(settings.daylight_now(0.0).is_none());
        assert_eq!(settings.effective_reference(None).value, 4200);
        assert!(settings.sky(0.0).is_none());
    }

    /// Enabled but with nowhere to stand is the state right after switching it on, before the
    /// position arrives. It has to behave as off rather than as midnight.
    #[test]
    fn a_curve_without_a_position_does_nothing() {
        let settings = with_curve(2.0, None);
        assert!(settings.daylight_now(1_784_000_000.0).is_none());
        assert_eq!(settings.effective_reference(None).value, 5000);
    }

    /// In full daylight there is nothing to correct for, whatever the factor.
    #[test]
    fn full_daylight_holds_the_stored_reference() {
        for factor in [1.0, 2.0, 8.0] {
            let settings = with_curve(factor, Some(BERLIN));
            assert_eq!(settings.effective_reference(Some(1.0)).value, 5000);
        }
    }

    /// The factor is a brightness ratio, so 2.0 has to come out as exactly one stop and 4.0 as
    /// exactly two. This is the whole contract of the setting.
    #[test]
    fn the_factor_is_a_brightness_ratio_at_night() {
        for (factor, expected_stops) in [(2.0, -1.0), (4.0, -2.0), (1.5, -0.585)] {
            let settings = with_curve(factor, Some(BERLIN));
            let night = settings.effective_reference(Some(0.0));
            let stops = night.stops_from(settings.reference).expect("both positive");
            assert!(
                (stops - expected_stops).abs() < 0.01,
                "factor {factor} gave {stops} stops, expected {expected_stops}"
            );
            // And the ratio itself, stated the way the setting is worded.
            let ratio = settings.reference.linear / night.linear;
            assert!(
                (ratio - factor).abs() < 0.01,
                "factor {factor} gave ratio {ratio}"
            );
        }
    }

    /// Factor 1.0 means "no difference between day and night", so it must be a no-op at every
    /// point on the curve - the second way to opt out, next to the toggle.
    #[test]
    fn a_factor_of_one_never_moves_the_reference() {
        let settings = with_curve(1.0, Some(BERLIN));
        for daylight in [0.0, 0.25, 0.5, 1.0] {
            assert_eq!(settings.effective_reference(Some(daylight)).value, 5000);
        }
    }

    /// A factor below 1.0 would brighten the sequence as the sky darkened. Refused at the door.
    #[tokio::test]
    async fn a_factor_below_one_is_pulled_up_to_one() {
        let state = RampState::default();
        let stored = state.set(with_curve(0.25, Some(BERLIN))).await;
        assert_eq!(stored.daylight.factor, 1.0);
    }

    /// Anchoring on a frame has to mean "hold what I am seeing", so the stored base and the
    /// daylight offset must cancel exactly.
    #[test]
    fn anchoring_on_a_frame_round_trips_through_the_curve() {
        let settings = with_curve(2.0, Some(BERLIN));
        let measured = Luminance::from_value(2269);

        for daylight in [0.0, 0.3, 0.75, 1.0] {
            let base = settings.base_from_measured(measured, Some(daylight));
            let held = RampSettings {
                reference: base,
                ..settings.clone()
            }
            .effective_reference(Some(daylight));

            assert!(
                (held.linear - measured.linear).abs() < 1e-6,
                "at {daylight} daylight the target came back as {} not {}",
                held.value,
                measured.value
            );
        }
    }

    /// The target may only ever fall as the sky darkens. A curve that wandered back up would
    /// show as a brightness bump in the finished sequence.
    #[test]
    fn the_target_falls_monotonically_with_the_light() {
        let settings = with_curve(3.0, Some(BERLIN));
        let mut previous = f32::MAX;
        for step in 0..=20 {
            let daylight = 1.0 - step as f32 / 20.0;
            let target = settings.effective_reference(Some(daylight)).linear;
            assert!(target <= previous, "target rose at {daylight} daylight");
            previous = target;
        }
    }

    /// Sunrise needs no separate handling: the curve tracks the sun, so running the same
    /// settings through a rising sun brightens the target on its own. Worth pinning down, since
    /// the obvious "fix" would be to special-case the mode and break it.
    #[test]
    fn the_same_curve_serves_sunrise_by_tracking_the_sun() {
        let settings = with_curve(2.0, Some(BERLIN));
        // Around midsummer sunrise in Berlin: 02:00 UTC is night, 04:00 is up.
        let night = utc_seconds(2026, 6, 21, 1.0);
        let morning = utc_seconds(2026, 6, 21, 5.0);

        let before = settings.sky(night).expect("curve is on");
        let after = settings.sky(morning).expect("curve is on");

        assert!(
            after.daylight > before.daylight,
            "daylight {} -> {}",
            before.daylight,
            after.daylight
        );
        assert!(
            after.effective_reference.linear > before.effective_reference.linear,
            "target {} -> {}",
            before.effective_reference.value,
            after.effective_reference.value
        );
    }

    /// The readout the UI shows has to agree with the number the engine acts on, or the app is
    /// explaining one thing and doing another.
    #[test]
    fn the_sky_readout_matches_the_effective_reference() {
        let settings = with_curve(2.0, Some(BERLIN));
        let when = utc_seconds(2026, 7, 28, 19.0);
        let sky = settings.sky(when).expect("curve is on");
        let (elevation, daylight) = settings.daylight_now(when).expect("curve is on");

        assert_eq!(sky.daylight, daylight);
        assert!((sky.elevation_degrees as f64 - elevation).abs() < 1e-4);
        assert_eq!(
            sky.effective_reference.value,
            settings.effective_reference(Some(daylight)).value
        );
        assert!(sky.offset_stops <= 0.0, "the curve may only darken");
    }

    /// Whole hours UTC, enough for tests that only need "night" or "morning".
    fn utc_seconds(year: i64, month: u32, day: u32, hour: f64) -> f64 {
        let mut days: i64 = 0;
        for y in 1970..year {
            days += if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                366
            } else {
                365
            };
        }
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let lengths = [
            31,
            if leap { 29 } else { 28 },
            31,
            30,
            31,
            30,
            31,
            31,
            30,
            31,
            30,
            31,
        ];
        days += lengths[..month as usize - 1].iter().sum::<i64>();
        days += day as i64 - 1;
        days as f64 * 86_400.0 + hour * 3_600.0
    }

    #[test]
    fn a_fresh_ramp_is_disarmed() {
        let settings = RampSettings::default();
        // Arming by default would let a ramp start moving a camera nobody aimed yet.
        assert!(!settings.active);
        assert_eq!(settings.reference.value, 5000);
    }

    /// The property the ramp is built on: the deviation *is* the correction. Measured
    /// through `Luminance::stops_from`, which is what the engine will call.
    #[test]
    fn deviation_is_measured_in_stops_from_the_reference() {
        let settings = RampSettings {
            reference: Luminance::from_linear(0.1),
            ..Default::default()
        };

        // A frame twice as bright is one stop over.
        let over = Luminance::from_linear(0.2)
            .stops_from(settings.reference)
            .unwrap();
        assert!((over - 1.0).abs() < 1e-4, "{over}");

        // Half as bright is one stop under.
        let under = Luminance::from_linear(0.05)
            .stops_from(settings.reference)
            .unwrap();
        assert!((under + 1.0).abs() < 1e-4, "{under}");

        // On target is zero, which is what tells the ramp to leave the camera alone.
        let on_target = settings.reference.stops_from(settings.reference).unwrap();
        assert!(on_target.abs() < 1e-6, "{on_target}");
    }

    #[test]
    fn a_black_frame_yields_no_deviation() {
        let settings = RampSettings::default();
        assert!(Luminance::from_linear(0.0)
            .stops_from(settings.reference)
            .is_none());
    }

    #[tokio::test]
    async fn settings_survive_being_read_back() {
        let state = RampState::default();
        assert!(!state.get().await.active);

        let stored = state
            .set(RampSettings {
                active: true,
                mode: RampMode::Sunrise,
                reference: Luminance::from_value(3200),
                ..Default::default()
            })
            .await;

        // The setter returns what was kept, so a caller never has to assume.
        assert!(stored.active);
        assert_eq!(stored.mode, RampMode::Sunrise);
        assert_eq!(stored.reference.value, 3200);
        assert_eq!(state.get().await, stored);
    }

    /// The spinner in the UI edits `value` alone. Storing that verbatim would leave
    /// `linear` describing the previous brightness, and `linear` is what the ramp computes
    /// with - so the displayed reference and the acted-on reference would part ways.
    #[tokio::test]
    async fn a_reference_edited_by_value_alone_is_made_consistent() {
        let state = RampState::default();

        let mismatched = RampSettings {
            reference: Luminance {
                value: 3200,
                // Deliberately the linear part of a different brightness.
                linear: Luminance::from_value(9000).linear,
            },
            ..Default::default()
        };
        let sent_linear = mismatched.reference.linear;
        let stored = state.set(mismatched).await;

        assert_eq!(stored.reference.value, 3200);
        assert_eq!(stored.reference.linear, Luminance::from_value(3200).linear);
        assert_ne!(stored.reference.linear, sent_linear);
    }

    /// Shutter and ISO ramp by default, aperture does not - a mechanical aperture steps
    /// coarsely and produces visible flicker, so leaving it alone is the usual answer.
    #[test]
    fn aperture_ramping_is_off_by_default() {
        let settings = RampSettings::default();
        assert!(settings.shutter.enabled);
        assert!(settings.iso.enabled);
        assert!(!settings.aperture.enabled);

        // No limits until someone picks one from the camera's own list.
        assert!(settings.shutter.limit.is_none());
        assert!(settings.aperture.limit.is_none());
        assert!(settings.iso.limit.is_none());
    }

    /// The three dials are configured independently, so a toggle on one must not disturb
    /// the others.
    #[tokio::test]
    async fn dials_are_configured_independently() {
        let state = RampState::default();

        let stored = state
            .set(RampSettings {
                aperture: DialRamp {
                    enabled: true,
                    limit: Some("280".into()),
                },
                ..Default::default()
            })
            .await;

        assert!(stored.aperture.enabled);
        assert_eq!(stored.aperture.limit.as_deref(), Some("280"));
        // Untouched fields keep the values they had.
        assert!(stored.shutter.enabled);
        assert!(stored.shutter.limit.is_none());
        assert!(stored.iso.enabled);
    }

    /// A limit is stored as the camera's own token, verbatim. Rewriting or parsing it here
    /// would be inventing a value the body never offered.
    #[tokio::test]
    async fn a_limit_is_kept_as_the_camera_wrote_it() {
        let state = RampState::default();
        let stored = state
            .set(RampSettings {
                shutter: DialRamp {
                    enabled: true,
                    limit: Some("300000".into()),
                },
                ..Default::default()
            })
            .await;

        assert_eq!(stored.shutter.limit.as_deref(), Some("300000"));
    }

    #[tokio::test]
    async fn pointing_the_reference_at_a_frame_leaves_the_rest_alone() {
        let state = RampState::default();
        state
            .set(RampSettings {
                active: true,
                mode: RampMode::Sunrise,
                ..Default::default()
            })
            .await;

        let stored = state.set_reference(Luminance::from_value(2269), 0.0).await;

        assert_eq!(stored.reference.value, 2269);
        // Taking a reading must not disarm the ramp or flip its direction.
        assert!(stored.active);
        assert_eq!(stored.mode, RampMode::Sunrise);
    }
}
