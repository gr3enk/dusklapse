//! Deciding what to change, without changing anything.
//!
//! A pure function over the numbers, deliberately separated from the code that talks to the
//! camera. This is the most consequential logic in the app - it moves someone's camera in the
//! middle of a sequence they cannot reshoot - and a decision that needs a camera on the desk
//! to exercise is a decision that does not get exercised.
//!
//! # One step per frame
//!
//! The correction is not applied all at once. However far the frame is from the reference, at
//! most one notch of one dial moves, and the sequence catches up over the following frames.
//!
//! This is the whole point of ramping rather than correcting. A dial jumping several stops
//! between two frames is a visible flash in the finished video - exactly the artefact the
//! technique exists to avoid. A third of a stop per frame is invisible, and at any sane
//! interval it still keeps up comfortably: dusk moves at roughly a stop every five to ten
//! minutes, while frames arrive every few seconds.
//!
//! The cost is that a reference set far from where the sequence currently sits takes a while
//! to reach. That is the right trade - smoothness is the product, speed is not.
//!
//! # Which dial moves
//!
//! The highest-priority dial that still has room. At sunset that is shutter, then aperture,
//! then ISO - open the shutter first because it costs only motion blur, and reach for ISO last
//! because it costs noise. At sunrise the light runs the other way and so does the order.
//!
//! # One direction only
//!
//! At sunset the ramp only ever brightens; at sunrise it only ever darkens. That matches how a
//! holy-grail sequence behaves - the sky moves one way, and chasing a frame that came out
//! *brighter* than the reference would mean following a car's headlights instead of the sun.
//!
//! It has a consequence worth stating plainly: a single dark frame - someone walking through
//! the shot - opens the exposure by a notch and it never comes back. Guarding against that
//! needs either a bidirectional correction or a rule that ignores a reading too far from its
//! neighbours, and both are decisions rather than details.

use serde::Serialize;

use crate::camera::{Dial, ExposureCapabilities, ExposureSettings, ExposureValue, Luminance};

use super::{RampMode, RampSettings};

/// Below this, leave the camera alone.
///
/// The finest step a body offers is a third of a stop, so a smaller deviation cannot be
/// corrected anyway. Acting on it would mean a write per frame that changes nothing, and each
/// write is a round trip that blocks the command channel.
const MIN_CORRECTION_STOPS: f32 = 0.05;

/// Why a dial could not be moved.
///
/// Carried through to the UI, because "out of headroom" on its own is not something anyone can
/// act on - what you need to know is *which* dial is stuck and at what.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Blocked {
    /// Its toggle is off.
    Disabled,
    /// Already sitting on the limit that was set for it.
    AtLimit { limit: String },
    /// The camera offers nothing further in this direction.
    EndOfRange,
    /// On bulb or auto, which has no stop position to move relative to.
    NoStopPosition,
    /// The limit that was set is not in the camera's current list - a lens or mode change.
    LimitUnavailable { limit: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedDial {
    pub dial: Dial,
    pub reason: Blocked,
}

/// One dial move, ready to be sent to the camera.
#[derive(Debug, Clone, PartialEq)]
pub struct DialChange {
    pub dial: Dial,
    /// The token currently set, kept for the log and for the UI.
    pub from: String,
    /// The token to send. Always one the camera itself reported.
    pub to: String,
    /// Brightness this move buys, in stops. Positive brightens.
    pub gained_stops: f32,
}

/// What the ramp decided about one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Correction {
    /// How far the frame was from the reference. Positive means brighter than wanted.
    pub deviation_stops: f32,
    /// The move to apply. At most one - see the module documentation.
    pub change: Option<DialChange>,
    /// Why each dial that was passed over could not be used.
    ///
    /// Populated only when nothing could move at all. When a dial does move, the ones ahead of
    /// it in priority order were simply lower priority, not blocked - saying otherwise would
    /// turn normal operation into a list of complaints.
    pub blocked: Vec<BlockedDial>,
}

/// The order dials are spent in, and therefore the priority.
fn dial_order(mode: RampMode) -> [Dial; 3] {
    match mode {
        RampMode::Sunset => [Dial::Shutter, Dial::Aperture, Dial::Iso],
        RampMode::Sunrise => [Dial::Iso, Dial::Aperture, Dial::Shutter],
    }
}

/// Which way this mode is allowed to move brightness: `+1` brighter, `-1` darker.
fn direction(mode: RampMode) -> f32 {
    match mode {
        RampMode::Sunset => 1.0,
        RampMode::Sunrise => -1.0,
    }
}

/// Work out what to change about one frame.
///
/// `None` when there is nothing to decide: the ramp is disarmed, or the frame's brightness
/// cannot be compared to the reference. `Some` with no change means either the frame is close
/// enough to leave alone, or nothing could move - which `blocked` explains.
///
/// `reference` is passed in rather than read from `settings` because the target moves: the
/// daylight curve walks it down as the sky darkens. Taking it as an argument keeps that one
/// decision in the caller instead of leaving this function to guess which reference applies.
pub fn plan(
    settings: &RampSettings,
    capabilities: &ExposureCapabilities,
    exposure: &ExposureSettings,
    frame: Luminance,
    reference: Luminance,
) -> Option<Correction> {
    if !settings.active {
        return None;
    }

    let deviation = frame.stops_from(reference)?;
    let dir = direction(settings.mode);
    // Stops of brightness to add. Negative darkens.
    let needed = -deviation;

    // Only correct in the direction the light is going, and only when it is worth a write.
    if needed * dir <= MIN_CORRECTION_STOPS {
        return Some(Correction {
            deviation_stops: deviation,
            change: None,
            blocked: Vec::new(),
        });
    }

    let mut blocked = Vec::new();

    for dial in dial_order(settings.mode) {
        match step(settings, capabilities, exposure, dial, dir) {
            Ok(change) => {
                return Some(Correction {
                    deviation_stops: deviation,
                    change: Some(change),
                    blocked: Vec::new(),
                })
            }
            Err(reason) => blocked.push(BlockedDial { dial, reason }),
        }
    }

    Some(Correction {
        deviation_stops: deviation,
        change: None,
        blocked,
    })
}

/// The next notch of one dial, or why there is not one.
///
/// Takes the direction but not the size of the correction: how far off the frame is decides
/// *whether* to act and is settled by the caller. Which dial and how far it may go is a matter of
/// the configured order and the limits, and nothing else.
fn step(
    settings: &RampSettings,
    capabilities: &ExposureCapabilities,
    exposure: &ExposureSettings,
    dial: Dial,
    dir: f32,
) -> Result<DialChange, Blocked> {
    let config = settings.dial(dial);
    if !config.enabled {
        return Err(Blocked::Disabled);
    }

    let current = exposure.dial(dial).ok_or(Blocked::NoStopPosition)?;
    let current_stops = current.stops.ok_or(Blocked::NoStopPosition)?;

    let limit_stops = match &config.limit {
        // Resolved against the *current* list rather than trusted: a limit chosen for another
        // lens or mode may no longer be offered, and inventing a stop position for it would let
        // the ramp run past a boundary someone set on purpose.
        Some(raw) => Some(
            resolve(capabilities.dial(dial), raw)
                .ok_or_else(|| Blocked::LimitUnavailable { limit: raw.clone() })?,
        ),
        None => None,
    };

    // The nearest value strictly further along in the ramp's direction, and not past the limit.
    let next = capabilities
        .dial(dial)
        .iter()
        .filter_map(|value| value.stops.map(|stops| (value, stops)))
        .filter(|(_, stops)| {
            let forward = if dir > 0.0 {
                *stops > current_stops + f32::EPSILON
            } else {
                *stops < current_stops - f32::EPSILON
            };
            let within = match limit_stops {
                Some(limit) if dir > 0.0 => *stops <= limit,
                Some(limit) => *stops >= limit,
                None => true,
            };
            forward && within
        })
        // Nearest first: one notch, not the biggest jump available.
        .min_by(|(_, a), (_, b)| {
            (a - current_stops)
                .abs()
                .partial_cmp(&(b - current_stops).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

    let Some((value, stops)) = next else {
        // Distinguishes "you told it to stop here" from "the camera has nothing left", because
        // the first is something you can change and the second is not.
        return Err(match &config.limit {
            Some(limit) => Blocked::AtLimit {
                limit: limit.clone(),
            },
            None => Blocked::EndOfRange,
        });
    };

    // No check on whether this notch is "worth" the correction. The order is the contract: the
    // first dial with room takes the step, however coarse its grid is.
    //
    // There used to be a guard here that refused a notch larger than twice the shortfall, on the
    // grounds that it lands further from the reference than staying put. It centred the error, and
    // it broke the thing that actually matters. Two ways, both seen in the field on a Z 6:
    //
    //  - Every dial refusing at once, which the UI reported as the ramp having run out of headroom.
    //  - Worse: the shutter refusing while ISO accepted, because the shutter grid is uneven
    //    (1/40 -> 1/30 is 0.415 EV) and ISO's is not (1000 -> 1250 is 0.322 EV). ISO climbed while
    //    the shutter still had three notches to its configured limit.
    //
    // The cost of dropping it is a brightness offset: a 0.06 EV shortfall answered with a 0.4 EV
    // notch sits ~0.35 EV bright until the light catches up. That is a level, not a flicker, and
    // it is the cheaper of the two errors.
    let gained = stops - current_stops;

    Ok(DialChange {
        dial,
        from: current.raw.clone(),
        to: value.raw.clone(),
        gained_stops: gained,
    })
}

/// Stop position of a raw token in the list the camera currently offers.
fn resolve(values: &[ExposureValue], raw: &str) -> Option<f32> {
    values.iter().find(|value| value.raw == raw)?.stops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ramp::DialRamp;

    fn shutter(raw: &str, seconds: f32) -> ExposureValue {
        ExposureValue {
            raw: raw.into(),
            label: raw.into(),
            stops: Some(seconds.log2()),
        }
    }
    fn aperture(raw: &str, f: f32) -> ExposureValue {
        ExposureValue {
            raw: raw.into(),
            label: raw.into(),
            stops: Some(-2.0 * f.log2()),
        }
    }
    fn iso(raw: &str, value: f32) -> ExposureValue {
        ExposureValue {
            raw: raw.into(),
            label: raw.into(),
            stops: Some((value / 100.0).log2()),
        }
    }

    /// A third-stop range around where the Z 6 actually sits at dusk.
    ///
    /// Whole stops hide this class of bug: with one-stop notches the half-notch guard only bites
    /// below 0.5 EV, which the other tests never approach. Third stops bite below 0.167 EV, which
    /// is most frames of a real sunset.
    fn third_stop_capabilities() -> ExposureCapabilities {
        ExposureCapabilities {
            shutter: vec![shutter("1600", 0.16)],
            aperture: vec![
                aperture("250", 2.5),
                aperture("220", 2.2),
                aperture("200", 2.0),
                aperture("180", 1.8),
            ],
            iso: vec![
                iso("250", 250.0),
                iso("320", 320.0),
                iso("400", 400.0),
                iso("500", 500.0),
            ],
        }
    }

    /// An exposure built from the third-stop list, since `exposure_at` reads the whole-stop one.
    fn third_stop_exposure(a: &str, i: &str) -> ExposureSettings {
        let caps = third_stop_capabilities();
        let find =
            |values: &[ExposureValue], raw: &str| values.iter().find(|v| v.raw == raw).cloned();
        ExposureSettings {
            shutter: find(&caps.shutter, "1600"),
            aperture: find(&caps.aperture, a),
            iso: find(&caps.iso, i),
        }
    }

    /// The dial list around 1/40s and ISO 1000, with the real third-stop values.
    ///
    /// The shutter row matters: 1/40 -> 1/30 is 0.415 EV, not 0.333. The nominal third-stop
    /// shutter sequence is uneven, and that unevenness is what broke the priority order.
    fn dusk_capabilities() -> ExposureCapabilities {
        ExposureCapabilities {
            shutter: vec![
                shutter("250", 1.0 / 40.0),
                shutter("333", 1.0 / 30.0),
                shutter("400", 1.0 / 25.0),
                shutter("500", 1.0 / 20.0),
            ],
            aperture: vec![aperture("250", 2.5)],
            iso: vec![
                iso("1000", 1000.0),
                iso("1250", 1250.0),
                iso("1600", 1600.0),
            ],
        }
    }

    fn dusk_exposure(s: &str, i: &str) -> ExposureSettings {
        let caps = dusk_capabilities();
        let find =
            |values: &[ExposureValue], raw: &str| values.iter().find(|v| v.raw == raw).cloned();
        ExposureSettings {
            shutter: find(&caps.shutter, s),
            aperture: find(&caps.aperture, "250"),
            iso: find(&caps.iso, i),
        }
    }

    /// ISO must not move while the shutter still has room, whatever the notch sizes are.
    ///
    /// The field report: 1/40s with a limit of 1/20s, and ISO climbing anyway. A 0.18 EV shortfall
    /// is under half the shutter's 0.415 EV notch but over half ISO's 0.322 EV one, so the old
    /// per-dial guard refused the shutter, accepted ISO, and quietly broke the configured order.
    #[test]
    fn the_shutter_is_used_before_iso_however_coarse_its_grid_is() {
        let settings = armed(RampMode::Sunset, on(Some("500")), off(), on(Some("1600")));
        let frame = frame_below(&settings, 0.18);

        let correction = plan(
            &settings,
            &dusk_capabilities(),
            &dusk_exposure("250", "1000"),
            frame,
            settings.reference,
        )
        .unwrap();

        let change = correction
            .change
            .expect("the shutter has three notches of headroom");
        assert_eq!(
            change.dial,
            Dial::Shutter,
            "ISO moved while the shutter had room"
        );
        assert_eq!(change.to, "333");
        assert!(correction.blocked.is_empty());
    }

    /// The shutter is walked all the way to its limit before anything else is touched.
    ///
    /// One notch per frame, so this steps 1/40 -> 1/30 -> 1/25 -> 1/20 and only then hands over.
    /// The whole point of the order, tested as a sequence rather than a single decision.
    #[test]
    fn the_shutter_is_walked_to_its_limit_before_iso_is_touched() {
        let settings = armed(RampMode::Sunset, on(Some("500")), off(), on(Some("1600")));
        let capabilities = dusk_capabilities();
        let frame = frame_below(&settings, 1.5);

        let mut shutter = "250".to_string();
        let mut steps = Vec::new();
        for _ in 0..3 {
            let correction = plan(
                &settings,
                &capabilities,
                &dusk_exposure(&shutter, "1000"),
                frame,
                settings.reference,
            )
            .unwrap();
            let change = correction.change.expect("shutter should keep stepping");
            assert_eq!(change.dial, Dial::Shutter);
            shutter = change.to.clone();
            steps.push(change.to);
        }
        assert_eq!(steps, vec!["333", "400", "500"]);

        // Now on the limit, and only now does ISO get a turn.
        let correction = plan(
            &settings,
            &capabilities,
            &dusk_exposure("500", "1000"),
            frame,
            settings.reference,
        )
        .unwrap();
        assert_eq!(correction.change.expect("iso takes over").dial, Dial::Iso);
    }

    /// A dial genuinely out of play still hands over, which is the behaviour the order relies on.
    /// Only *waiting* stops the search.
    #[test]
    fn a_shutter_at_its_limit_still_hands_over_to_iso() {
        let settings = armed(RampMode::Sunset, on(Some("500")), off(), on(Some("1600")));
        let frame = frame_below(&settings, 0.5);

        let correction = plan(
            &settings,
            &dusk_capabilities(),
            // Already sitting on the 1/20s limit.
            &dusk_exposure("500", "1000"),
            frame,
            settings.reference,
        )
        .unwrap();

        let change = correction.change.expect("iso should take over");
        assert_eq!(change.dial, Dial::Iso);
    }

    /// A small shortfall on a third-stop body corrects rather than reporting a limit.
    ///
    /// The first field report this comes from: f/2.5 and ISO 250 with limits of f/1.8 and ISO 5000,
    /// and the UI announcing "nothing left to adjust" on both dials. Nothing was at a limit.
    #[test]
    fn a_small_shortfall_corrects_instead_of_reporting_a_limit() {
        let settings = armed(RampMode::Sunset, off(), on(Some("180")), on(Some("500")));
        let frame = frame_below(&settings, 0.1);

        let correction = plan(
            &settings,
            &third_stop_capabilities(),
            &third_stop_exposure("250", "250"),
            frame,
            settings.reference,
        )
        .unwrap();

        // Aperture, because it comes before ISO in sunset order and had room.
        let change = correction.change.expect("aperture had three notches left");
        assert_eq!(change.dial, Dial::Aperture);
        assert_eq!(change.to, "220");
        assert!(correction.blocked.is_empty(), "{:?}", correction.blocked);
    }

    /// Whole-stop ranges, so a "one notch" expectation is exactly one stop and easy to read.
    fn capabilities() -> ExposureCapabilities {
        ExposureCapabilities {
            shutter: vec![
                shutter("100", 0.01),
                shutter("200", 0.02),
                shutter("400", 0.04),
                shutter("800", 0.08),
                shutter("1600", 0.16),
                shutter("3200", 0.32),
                shutter("6400", 0.64),
                shutter("12800", 1.28),
                shutter("25600", 2.56),
                ExposureValue {
                    raw: "4294967295".into(),
                    label: "BULB".into(),
                    stops: None,
                },
            ],
            aperture: vec![
                aperture("180", 1.8),
                aperture("280", 2.8),
                aperture("400", 4.0),
                aperture("560", 5.6),
            ],
            iso: vec![
                iso("100", 100.0),
                iso("200", 200.0),
                iso("400", 400.0),
                iso("800", 800.0),
                iso("1600", 1600.0),
                iso("3200", 3200.0),
            ],
        }
    }

    fn exposure_at(s: &str, a: &str, i: &str) -> ExposureSettings {
        let caps = capabilities();
        let find =
            |values: &[ExposureValue], raw: &str| values.iter().find(|v| v.raw == raw).cloned();
        ExposureSettings {
            shutter: find(&caps.shutter, s),
            aperture: find(&caps.aperture, a),
            iso: find(&caps.iso, i),
        }
    }

    fn frame_below(settings: &RampSettings, stops: f32) -> Luminance {
        Luminance::from_linear(settings.reference.linear * 2f32.powf(-stops))
    }

    fn armed(mode: RampMode, shutter: DialRamp, aperture: DialRamp, iso: DialRamp) -> RampSettings {
        RampSettings {
            active: true,
            mode,
            reference: Luminance::from_value(5000),
            shutter,
            aperture,
            iso,
            daylight: Default::default(),
        }
    }
    fn on(limit: Option<&str>) -> DialRamp {
        DialRamp {
            enabled: true,
            limit: limit.map(str::to_string),
        }
    }
    fn off() -> DialRamp {
        DialRamp {
            enabled: false,
            limit: None,
        }
    }

    #[test]
    fn a_disarmed_ramp_decides_nothing() {
        let settings = RampSettings::default();
        let frame = frame_below(&settings, 2.0);
        assert!(plan(
            &settings,
            &capabilities(),
            &exposure_at("1600", "280", "400"),
            frame,
            settings.reference
        )
        .is_none());
    }

    #[test]
    fn a_frame_on_target_is_left_alone() {
        let settings = armed(RampMode::Sunset, on(None), off(), on(None));
        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("1600", "280", "400"),
            settings.reference,
            settings.reference,
        )
        .unwrap();
        assert!(correction.change.is_none());
        assert!(correction.blocked.is_empty());
    }

    /// The property this module exists for: however far off the frame is, one notch moves.
    /// A dial jumping several stops between frames is a visible flash in the finished video.
    #[test]
    fn a_large_deviation_still_moves_only_one_notch() {
        let settings = armed(RampMode::Sunset, on(None), off(), on(None));
        // Five stops dark, but the shutter may only advance one step.
        let frame = frame_below(&settings, 5.0);

        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("1600", "280", "400"),
            frame,
            settings.reference,
        )
        .unwrap();
        let change = correction.change.expect("a move");

        assert_eq!(change.dial, Dial::Shutter);
        // 0.16 s to 0.32 s, exactly one stop - not the four the deviation would justify.
        assert_eq!(change.to, "3200");
        assert!(
            (change.gained_stops - 1.0).abs() < 1e-4,
            "{}",
            change.gained_stops
        );
    }

    /// Priority order, and only one dial per frame.
    #[test]
    fn sunset_takes_the_shutter_before_anything_else() {
        let settings = armed(RampMode::Sunset, on(None), on(None), on(None));
        let frame = frame_below(&settings, 2.0);

        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("1600", "280", "400"),
            frame,
            settings.reference,
        )
        .unwrap();
        assert_eq!(correction.change.unwrap().dial, Dial::Shutter);
    }

    /// The dial the user's test hit: shutter and aperture off, so ISO is the only one left and
    /// it steps up one value.
    #[test]
    fn with_only_iso_enabled_iso_steps_up() {
        let settings = armed(RampMode::Sunset, off(), off(), on(Some("3200")));
        let frame = frame_below(&settings, 3.31);

        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("320", "180", "400"),
            frame,
            settings.reference,
        )
        .unwrap();
        let change = correction.change.expect("a move");

        assert_eq!(change.dial, Dial::Iso);
        // ISO 400 to 800: the next value up, not a leap to the limit.
        assert_eq!(change.to, "800");
    }

    /// Once the shutter has nowhere to go, the next dial in the order takes the notch.
    #[test]
    fn a_dial_at_its_limit_hands_over_to_the_next() {
        let settings = armed(RampMode::Sunset, on(Some("1600")), off(), on(None));
        let frame = frame_below(&settings, 2.0);

        // Shutter already sits on its own limit.
        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("1600", "280", "400"),
            frame,
            settings.reference,
        )
        .unwrap();
        let change = correction.change.expect("a move");

        assert_eq!(change.dial, Dial::Iso);
        assert_eq!(change.to, "800");
    }

    /// Exactly the situation that produced the confusing message: the only enabled dial is
    /// already on its limit. The reason has to name the dial and the limit.
    #[test]
    fn nothing_to_move_says_which_dial_and_which_limit() {
        let settings = armed(RampMode::Sunset, off(), off(), on(Some("1600")));
        let frame = frame_below(&settings, 3.31);

        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("320", "180", "1600"),
            frame,
            settings.reference,
        )
        .unwrap();

        assert!(correction.change.is_none());
        let iso = correction
            .blocked
            .iter()
            .find(|entry| entry.dial == Dial::Iso)
            .unwrap();
        assert_eq!(
            iso.reason,
            Blocked::AtLimit {
                limit: "1600".into()
            }
        );
        // And the two switched-off dials are reported as switched off, not as exhausted.
        for dial in [Dial::Shutter, Dial::Aperture] {
            let entry = correction
                .blocked
                .iter()
                .find(|entry| entry.dial == dial)
                .unwrap();
            assert_eq!(entry.reason, Blocked::Disabled);
        }
    }

    /// "You told it to stop here" and "the camera has nothing left" are different problems -
    /// the first is something you can change.
    #[test]
    fn the_end_of_a_range_is_not_reported_as_a_limit() {
        let settings = armed(RampMode::Sunset, on(None), off(), off());
        let frame = frame_below(&settings, 2.0);

        // Already on the longest exposure the camera lists, with no limit set.
        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("25600", "280", "400"),
            frame,
            settings.reference,
        )
        .unwrap();

        let shutter = correction
            .blocked
            .iter()
            .find(|entry| entry.dial == Dial::Shutter)
            .unwrap();
        assert_eq!(shutter.reason, Blocked::EndOfRange);
    }

    #[test]
    fn a_disabled_dial_is_never_moved() {
        let settings = armed(RampMode::Sunset, off(), off(), off());
        let frame = frame_below(&settings, 4.0);

        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("1600", "280", "400"),
            frame,
            settings.reference,
        )
        .unwrap();
        assert!(correction.change.is_none());
        assert_eq!(correction.blocked.len(), 3);
    }

    /// Sunrise darkens, and starts at the other end of the order.
    #[test]
    fn sunrise_darkens_starting_with_iso() {
        let settings = armed(RampMode::Sunrise, on(Some("100")), off(), on(Some("100")));
        // Two stops brighter than the reference is what triggers a sunrise correction.
        let frame = Luminance::from_linear(settings.reference.linear * 4.0);

        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("6400", "280", "1600"),
            frame,
            settings.reference,
        )
        .unwrap();
        let change = correction.change.expect("a move");

        assert_eq!(change.dial, Dial::Iso);
        // One notch down, not a leap to the limit of 100.
        assert_eq!(change.to, "800");
        assert!(change.gained_stops < 0.0);
    }

    /// A sunset ramp must not darken: a frame brighter than the reference is a headlight, not
    /// the sun.
    #[test]
    fn sunset_ignores_a_frame_that_came_out_brighter() {
        let settings = armed(RampMode::Sunset, on(None), off(), on(None));
        let frame = Luminance::from_linear(settings.reference.linear * 2.0);

        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("1600", "280", "400"),
            frame,
            settings.reference,
        )
        .unwrap();
        assert!(correction.change.is_none());
        assert!(
            correction.blocked.is_empty(),
            "not blocked - just nothing to do"
        );
        assert!(correction.deviation_stops > 0.0);
    }

    #[test]
    fn bulb_is_never_chosen_and_never_ramped_from() {
        let settings = armed(RampMode::Sunset, on(None), off(), off());
        let frame = frame_below(&settings, 8.0);

        // Sitting on the longest real value, bulb must not be the next notch.
        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("25600", "280", "400"),
            frame,
            settings.reference,
        )
        .unwrap();
        assert!(correction.change.is_none());

        // And a shutter already on bulb cannot be reasoned about.
        let on_bulb = ExposureSettings {
            shutter: Some(ExposureValue {
                raw: "4294967295".into(),
                label: "BULB".into(),
                stops: None,
            }),
            ..exposure_at("1600", "280", "400")
        };
        let correction = plan(
            &settings,
            &capabilities(),
            &on_bulb,
            frame,
            settings.reference,
        )
        .unwrap();
        let shutter = correction
            .blocked
            .iter()
            .find(|entry| entry.dial == Dial::Shutter)
            .unwrap();
        assert_eq!(shutter.reason, Blocked::NoStopPosition);
    }

    /// A limit the camera no longer offers takes its dial out of play rather than being read as
    /// "unlimited", and says so.
    #[test]
    fn an_unresolvable_limit_takes_its_dial_out_of_play() {
        let settings = armed(RampMode::Sunset, on(Some("999999")), off(), off());
        let frame = frame_below(&settings, 2.0);

        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("1600", "280", "400"),
            frame,
            settings.reference,
        )
        .unwrap();
        assert!(correction.change.is_none());
        let shutter = correction
            .blocked
            .iter()
            .find(|entry| entry.dial == Dial::Shutter)
            .unwrap();
        assert_eq!(
            shutter.reason,
            Blocked::LimitUnavailable {
                limit: "999999".into()
            }
        );
    }

    #[test]
    fn a_deviation_below_one_step_changes_nothing() {
        let settings = armed(RampMode::Sunset, on(None), off(), on(None));
        let frame = frame_below(&settings, 0.02);

        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("1600", "280", "400"),
            frame,
            settings.reference,
        )
        .unwrap();
        assert!(correction.change.is_none());
    }

    /// A notch coarser than the shortfall is taken anyway.
    ///
    /// The inverse of what this used to assert. Refusing it kept the error centred on the
    /// reference, at the price of the configured dial order - so the order won.
    #[test]
    fn a_notch_coarser_than_the_shortfall_is_still_taken() {
        let settings = armed(RampMode::Sunset, on(None), off(), off());
        // A tenth of a stop out, against a one-stop grid: the step overshoots ninefold.
        let frame = frame_below(&settings, 0.1);

        let correction = plan(
            &settings,
            &capabilities(),
            &exposure_at("1600", "280", "400"),
            frame,
            settings.reference,
        )
        .unwrap();
        let change = correction.change.expect("the shutter should still step");
        assert_eq!(change.dial, Dial::Shutter);
    }

    /// The whole chain against a real [`crate::camera::Camera`], not a fixture: read the value
    /// lists and the exposure off a camera, plan, send, and read back what it reports.
    ///
    /// The simulator refuses values outside its own ability list exactly as a body does, so this
    /// catches the class of bug a numbers-only test cannot: a plan that is arithmetically right
    /// and chooses a token no camera would accept.
    #[tokio::test]
    async fn a_correction_actually_moves_a_camera() {
        use crate::camera::{CameraTarget, Vendor};

        let camera = crate::camera::connect(CameraTarget::new(Vendor::Mock, "mock", 0))
            .await
            .unwrap();
        let capabilities = camera.capabilities().await.unwrap();
        let before = camera.exposure().await.unwrap();
        let frame = camera
            .preview()
            .await
            .unwrap()
            .and_then(|preview| preview.analysis)
            .map(|analysis| analysis.luminance)
            .expect("the simulator measures its frame");

        let settings = RampSettings {
            active: true,
            mode: RampMode::Sunset,
            // Two stops brighter than the frame, so the ramp has to open up.
            reference: Luminance::from_linear(frame.linear * 4.0),
            shutter: DialRamp {
                enabled: true,
                limit: Some("1/8".into()),
            },
            aperture: DialRamp {
                enabled: false,
                limit: None,
            },
            iso: DialRamp {
                enabled: true,
                limit: Some("6400".into()),
            },
            daylight: Default::default(),
        };

        let correction =
            plan(&settings, &capabilities, &before, frame, settings.reference).expect("a plan");
        let change = correction
            .change
            .expect("two stops down and nothing to do?");

        camera
            .set_exposure(change.dial, &change.to)
            .await
            .unwrap_or_else(|err| {
                panic!("camera refused {:?} = {}: {err}", change.dial, change.to)
            });

        // The camera has to report what the plan chose - proof the token was its own vocabulary
        // and not something invented.
        let after = camera.exposure().await.unwrap();
        assert_eq!(
            after.dial(change.dial).map(|value| value.raw.as_str()),
            Some(change.to.as_str())
        );

        // It really did get brighter, and by exactly what was planned.
        let gained = after.total_stops().unwrap() - before.total_stops().unwrap();
        assert!(
            (gained - change.gained_stops).abs() < 1e-3,
            "camera moved {gained}, plan said {}",
            change.gained_stops
        );

        // One notch, not the full two stops. The bound is loose because a notch is whatever the
        // camera's own list makes it - nominal whole stops are not exact powers of two, so this
        // list steps 1.06 stops between 1/125 and 1/60.
        assert!(gained > 0.0, "gained {gained}");
        assert!(
            gained < 1.5,
            "moved {gained} stops in one frame, which is a leap not a notch"
        );
    }
}
