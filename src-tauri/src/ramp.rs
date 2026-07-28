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

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::camera::Luminance;

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
            ..settings
        };
        guard.clone()
    }

    /// Point the reference at a brightness that was just measured.
    pub async fn set_reference(&self, reference: Luminance) -> RampSettings {
        let mut guard = self.0.write().await;
        guard.reference = reference;
        guard.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let over = Luminance::from_linear(0.2).stops_from(settings.reference).unwrap();
        assert!((over - 1.0).abs() < 1e-4, "{over}");

        // Half as bright is one stop under.
        let under = Luminance::from_linear(0.05).stops_from(settings.reference).unwrap();
        assert!((under + 1.0).abs() < 1e-4, "{under}");

        // On target is zero, which is what tells the ramp to leave the camera alone.
        let on_target = settings.reference.stops_from(settings.reference).unwrap();
        assert!(on_target.abs() < 1e-6, "{on_target}");
    }

    #[test]
    fn a_black_frame_yields_no_deviation() {
        let settings = RampSettings::default();
        assert!(Luminance::from_linear(0.0).stops_from(settings.reference).is_none());
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

        let stored = state.set_reference(Luminance::from_value(2269)).await;

        assert_eq!(stored.reference.value, 2269);
        // Taking a reading must not disarm the ramp or flip its direction.
        assert!(stored.active);
        assert_eq!(stored.mode, RampMode::Sunrise);
    }
}
