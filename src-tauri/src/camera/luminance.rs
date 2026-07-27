//! Scene brightness as a single number, for driving a ramp.
//!
//! # Why this is not the luma the histogram shows
//!
//! The histogram plots *luma* - the weighted sum of the gamma-encoded sRGB values,
//! which is what a camera's own histogram shows and what you judge by eye. That is
//! the wrong quantity to regulate on, because gamma encoding is non-linear: opening
//! up by a stop does not move encoded luma by a fixed amount, it moves it by an
//! amount that depends on where you already were.
//!
//! This module undoes the sRGB transfer function first and measures *relative
//! luminance* in linear light. That one change is what makes the metric useful for
//! control: doubling the light doubles the number, so the correction a ramp needs is
//! exactly `log2(target / current)` stops - one subtraction, no gain tuning, no
//! hunting around the setpoint.
//!
//! The Rec. 709 weights below are in fact *defined* for linear light. Applying them
//! to encoded values, as the histogram does, is a deliberate approximation that is
//! right for display and wrong for photometry.
//!
//! # The scale
//!
//! Reported on 0..10000 with mid-grey near 5000, matching what qDslrDashboard puts on
//! screen. The wide range is not decoration: on a 0..255 scale a tenth of a stop near
//! mid-grey is about nine units and lands on integers, which is too coarse to regulate
//! against. Here the same tenth of a stop is roughly 350 units.
//!
//! # The limitation worth knowing
//!
//! This measures the whole frame, so a large dark foreground pulls it down and a
//! bright moon pushes it up. That is inherent to any whole-frame metric, including
//! qDslrDashboard's, and it is why the reference value is something you set by looking
//! at a good frame of *your* composition rather than a universal constant. Metering
//! only part of the frame is the real fix and belongs with the ramp UI.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// Rec. 709 luminance weights, applied to linear light as they are defined for.
const WEIGHT_RED: f32 = 0.2126;
const WEIGHT_GREEN: f32 = 0.7152;
const WEIGHT_BLUE: f32 = 0.0722;

/// Top of the reported scale. Mid-grey lands near half of it.
pub const SCALE: f32 = 10000.0;

/// Floor added inside the logarithm.
///
/// A log-average has to do something about black pixels, whose logarithm is negative
/// infinity. The size of the floor is a real trade-off, not a formality: too small and
/// sensor noise in near-black pixels jitters the number frame to frame, which would
/// make a ramp hunt; too large and it stops distinguishing a dark scene from a very
/// dark one. 0.002 is roughly sRGB value 13 - below anything a photograph carries
/// information in.
const BLACK_FLOOR: f32 = 0.002;

/// Scene brightness, measured from a preview frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Luminance {
    /// Log-average relative luminance in linear light, 0.0 to 1.0.
    ///
    /// This is the field to compute with. It is proportional to the light that reached
    /// the sensor, so ratios of it are exposure differences.
    pub linear: f32,
    /// The same brightness on the 0..10000 scale, for display and for entering a
    /// reference by hand.
    pub value: u32,
}

impl Luminance {
    pub fn from_linear(linear: f32) -> Self {
        let linear = linear.clamp(0.0, 1.0);
        Self {
            linear,
            value: (encode_srgb(linear) * SCALE).round().clamp(0.0, SCALE) as u32,
        }
    }

    /// Rebuild from a displayed value, for a reference the user typed in.
    pub fn from_value(value: u32) -> Self {
        let encoded = (value as f32 / SCALE).clamp(0.0, 1.0);
        Self {
            linear: decode_srgb(encoded),
            value: value.min(SCALE as u32),
        }
    }

    /// How far this frame is from a reference, in stops.
    ///
    /// Positive means brighter than the reference. This is the whole point of
    /// measuring in linear light: the answer is the correction, directly - a ramp adds
    /// `-stops_from(reference)` to its exposure and is done, with no per-scene gain to
    /// tune.
    ///
    /// `None` when either side is black, where a ratio has no meaning.
    pub fn stops_from(&self, reference: Luminance) -> Option<f32> {
        if self.linear <= 0.0 || reference.linear <= 0.0 {
            return None;
        }
        Some((self.linear / reference.linear).log2())
    }
}

/// Accumulates luminance over a frame without holding the frame.
pub struct Meter {
    log_sum: f64,
    samples: u64,
}

impl Meter {
    pub fn new() -> Self {
        Self {
            log_sum: 0.0,
            samples: 0,
        }
    }

    /// Add one gamma-encoded sRGB pixel.
    pub fn add_rgb(&mut self, red: u8, green: u8, blue: u8) {
        let table = linear_table();
        let luminance = WEIGHT_RED * table[red as usize]
            + WEIGHT_GREEN * table[green as usize]
            + WEIGHT_BLUE * table[blue as usize];
        self.add_linear(luminance);
    }

    /// Add one gamma-encoded greyscale pixel.
    pub fn add_grey(&mut self, value: u8) {
        self.add_linear(linear_table()[value as usize]);
    }

    fn add_linear(&mut self, luminance: f32) {
        self.log_sum += ((luminance + BLACK_FLOOR) as f64).ln();
        self.samples += 1;
    }

    /// The log-average of everything added.
    ///
    /// Geometric rather than arithmetic mean, because exposure is multiplicative: a
    /// geometric mean shifts by exactly one stop when the light doubles, while an
    /// arithmetic mean shifts by an amount that depends on the distribution. The floor
    /// is subtracted back out so a uniform frame reports exactly its own brightness.
    pub fn finish(&self) -> Option<Luminance> {
        if self.samples == 0 {
            return None;
        }
        let mean = (self.log_sum / self.samples as f64).exp() as f32;
        Some(Luminance::from_linear((mean - BLACK_FLOOR).max(0.0)))
    }
}

impl Default for Meter {
    fn default() -> Self {
        Self::new()
    }
}

/// sRGB electro-optical transfer function: encoded 0..1 to linear 0..1.
fn decode_srgb(encoded: f32) -> f32 {
    if encoded <= 0.04045 {
        encoded / 12.92
    } else {
        ((encoded + 0.055) / 1.055).powf(2.4)
    }
}

/// The inverse, for putting a linear value back on a display scale.
fn encode_srgb(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// Linearised value for every possible 8-bit input.
///
/// A lookup table rather than a `powf` per channel per pixel: a megapixel frame would
/// otherwise mean three million calls to `powf`, which is measurable on a phone for a
/// result that only has 256 possible inputs.
fn linear_table() -> &'static [f32; 256] {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [0.0f32; 256];
        for (value, slot) in table.iter_mut().enumerate() {
            *slot = decode_srgb(value as f32 / 255.0);
        }
        table
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, tolerance: f32) {
        assert!((a - b).abs() <= tolerance, "{a} != {b} (within {tolerance})");
    }

    #[test]
    fn mid_grey_reads_near_the_middle_of_the_scale() {
        let mut meter = Meter::new();
        // sRGB 128 is the mid-grey a camera meters for.
        for _ in 0..100 {
            meter.add_rgb(128, 128, 128);
        }
        let luminance = meter.finish().unwrap();

        // Half of the scale, give or take the black floor.
        assert!(
            (4900..=5100).contains(&luminance.value),
            "mid-grey reported {}",
            luminance.value
        );
    }

    #[test]
    fn a_uniform_frame_reports_its_own_brightness() {
        // The floor is subtracted back out, so this must round-trip rather than come
        // back systematically dark.
        for encoded in [40u8, 90, 128, 200, 250] {
            let mut meter = Meter::new();
            meter.add_grey(encoded);
            let luminance = meter.finish().unwrap();
            approx(luminance.linear, decode_srgb(encoded as f32 / 255.0), 0.002);
        }
    }

    /// The property the whole module exists for: linear in stops.
    #[test]
    fn doubling_the_light_is_exactly_one_stop() {
        let dim = Luminance::from_linear(0.1);
        let bright = Luminance::from_linear(0.2);

        approx(bright.stops_from(dim).unwrap(), 1.0, 1e-4);
        approx(dim.stops_from(bright).unwrap(), -1.0, 1e-4);
        // And a frame is zero stops from itself.
        approx(dim.stops_from(dim).unwrap(), 0.0, 1e-6);
    }

    /// Encoded luma would fail this: the same stop change would move it by different
    /// amounts depending on where it started.
    #[test]
    fn a_stop_is_a_stop_wherever_you_are_on_the_scale() {
        for start in [0.02f32, 0.08, 0.2, 0.45] {
            let before = Luminance::from_linear(start);
            let after = Luminance::from_linear(start * 2.0);
            approx(after.stops_from(before).unwrap(), 1.0, 1e-4);
        }
    }

    #[test]
    fn a_reference_typed_in_by_hand_round_trips() {
        for value in [1000u32, 2500, 4000, 5000, 7000, 9500] {
            let reference = Luminance::from_value(value);
            assert_eq!(reference.value, value);
            // And measuring that exact brightness must agree with it.
            let measured = Luminance::from_linear(reference.linear);
            assert!(
                measured.value.abs_diff(value) <= 1,
                "{value} came back as {}",
                measured.value
            );
        }
    }

    #[test]
    fn green_carries_most_of_the_luminance() {
        let mut green = Meter::new();
        green.add_rgb(0, 255, 0);
        let mut blue = Meter::new();
        blue.add_rgb(0, 0, 255);

        assert!(green.finish().unwrap().linear > blue.finish().unwrap().linear * 5.0);
    }

    #[test]
    fn black_and_white_sit_at_the_ends() {
        let mut black = Meter::new();
        black.add_rgb(0, 0, 0);
        assert_eq!(black.finish().unwrap().value, 0);

        let mut white = Meter::new();
        white.add_rgb(255, 255, 255);
        assert_eq!(white.finish().unwrap().value, SCALE as u32);
    }

    #[test]
    fn a_ratio_against_black_has_no_answer() {
        let black = Luminance::from_linear(0.0);
        let grey = Luminance::from_linear(0.2);
        assert!(grey.stops_from(black).is_none());
        assert!(black.stops_from(grey).is_none());
    }

    #[test]
    fn an_empty_meter_measures_nothing() {
        assert!(Meter::new().finish().is_none());
    }

    /// A dark foreground pulling the number down is the known limitation, and it has to
    /// be a documented property rather than a surprise.
    #[test]
    fn a_dark_half_pulls_the_whole_frame_down() {
        let mut uniform = Meter::new();
        let mut half_dark = Meter::new();
        for _ in 0..100 {
            uniform.add_grey(128);
            half_dark.add_grey(128);
        }
        for _ in 0..100 {
            half_dark.add_grey(0);
        }

        assert!(half_dark.finish().unwrap().value < uniform.finish().unwrap().value);
    }
}
