//! Writing to a camera that is in the middle of an exposure.
//!
//! # The problem
//!
//! A body with the shutter open refuses everything else. PTP answers `Device_Busy` (0x2019), and
//! until this module existed that surfaced as "the camera refused the change" - which reads like a
//! value the body would not accept, when in fact the value was fine and only the moment was wrong.
//!
//! It is not a rare corner either. The ramp corrects on the *analysed* frame, and analysis comes
//! after the JPEG has crossed the network - a second or three. At an eight second interval with a
//! six second exposure there are two seconds of darkness per cycle, and the correction lands
//! squarely in the next exposure rather than in the gap.
//!
//! # The approach
//!
//! Ask again rather than model the shoot. Working out when the dark time falls would mean tracking
//! when each exposure started, guessing how long the body spends writing, and staying right about
//! both across dropped frames and reconnects - a model with several ways to be quietly wrong.
//!
//! The camera already knows the answer and gives it away for free: `Device_Busy` means "not yet",
//! anything else means the write happened. So try, wait a moment, try again, and stop when the
//! exposure can no longer be running. That is self-correcting - it lands in the dark time
//! wherever the dark time happens to be, without knowing where that is.
//!
//! The bound comes from the shutter speed, because an exposure is the one thing that reliably
//! makes a body say this, and its length is exactly how long the answer can stay "not yet".

use std::future::Future;
use std::time::Duration;

use super::error::{CameraError, CameraResult};
use super::model::{Dial, ExposureSettings};
use super::Camera;

/// How long to leave the camera alone between attempts.
///
/// Short enough that a two second gap still gets several chances, long enough not to be the kind
/// of hammering that upsets an older body - a refused write is cheap, but it is not free.
const RETRY_INTERVAL: Duration = Duration::from_millis(300);

/// Time allowed on top of the exposure itself.
///
/// A body is not ready the instant the shutter closes; it still has a frame to write. This is
/// slack for that, not a guess at how long it takes.
const MARGIN: Duration = Duration::from_secs(2);

/// The longest wait, whatever the shutter says.
///
/// Past 30 seconds a Nikon is in bulb or time, where the exposure ends when someone decides it
/// does and no budget could be right. Stopping and saying so beats waiting indefinitely.
const MAX_BUDGET: Duration = Duration::from_secs(35);

/// The wait when the shutter speed is not known.
///
/// Long enough to cover the exposures a timelapse spends most of its frames at, short enough that
/// a person who just turned a dial does not think the app has hung.
pub const UNKNOWN_SHUTTER_BUDGET: Duration = Duration::from_secs(8);

/// How long a write should keep trying, given what the camera is set to.
///
/// `None` for bulb and time, where the shutter carries no duration - those fall back to
/// [`UNKNOWN_SHUTTER_BUDGET`] rather than to something invented.
pub fn budget_for(exposure: &ExposureSettings) -> Duration {
    match shutter_duration(exposure) {
        Some(shutter) => (shutter + MARGIN).min(MAX_BUDGET),
        None => UNKNOWN_SHUTTER_BUDGET,
    }
}

/// The current exposure time, recovered from the stop value the dial carries.
///
/// `stops` is `log2(seconds)` for the shutter - see `exposure::shutter_stops` - so raising two to
/// it gives the seconds back exactly. Sentinels such as BULB carry no stops and yield `None`.
fn shutter_duration(exposure: &ExposureSettings) -> Option<Duration> {
    let stops = exposure.shutter.as_ref()?.stops?;
    let seconds = stops.exp2();
    // A negative or absurd value would mean the stop arithmetic is broken somewhere upstream;
    // silently turning it into a wait is worse than falling back to the default.
    (seconds.is_finite() && (0.0..=3600.0).contains(&seconds))
        .then(|| Duration::from_secs_f32(seconds))
}

/// Set a dial, waiting out an exposure if the body is mid-frame.
pub async fn set_exposure_when_ready(
    camera: &dyn Camera,
    dial: Dial,
    value: &str,
    budget: Duration,
) -> CameraResult<()> {
    while_busy(budget, || camera.set_exposure(dial, value)).await
}

/// Run `attempt` until it stops answering [`CameraError::Busy`], or until `budget` is spent.
///
/// Generic over the attempt rather than tied to one operation, so the retry rule can be tested
/// without a camera and reused for any other write a busy body might turn away.
pub async fn while_busy<F, Fut>(budget: Duration, mut attempt: F) -> CameraResult<()>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = CameraResult<()>>,
{
    let deadline = tokio::time::Instant::now() + budget;
    let mut waited = false;

    loop {
        match attempt().await {
            Err(CameraError::Busy) => {
                // Checked after the attempt, so a budget of zero still tries exactly once: a
                // caller asking for no patience is asking not to wait, not to skip the write.
                if tokio::time::Instant::now() + RETRY_INTERVAL > deadline {
                    log::warn!(
                        "camera stayed busy for {:.1}s; giving up on this change",
                        budget.as_secs_f32()
                    );
                    return Err(CameraError::Busy);
                }
                if !waited {
                    waited = true;
                    // Once per write, not once per attempt: the log view is read by a person and
                    // a line every 300ms would bury everything around it.
                    log::info!("camera is mid-exposure; waiting for the gap between frames");
                }
                tokio::time::sleep(RETRY_INTERVAL).await;
            }
            other => {
                if waited {
                    log::info!("camera free again; change applied in the gap");
                }
                return other;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::camera::ExposureValue;

    fn settings(stops: Option<f32>) -> ExposureSettings {
        ExposureSettings {
            shutter: Some(ExposureValue {
                raw: "x".into(),
                label: "x".into(),
                stops,
            }),
            aperture: None,
            iso: None,
        }
    }

    #[test]
    fn the_budget_covers_the_exposure_and_then_some() {
        // 6s exposure, the case that produced the bug: 8s interval, 2s of darkness.
        let budget = budget_for(&settings(Some(6f32.log2())));
        assert!(
            budget >= Duration::from_secs(6),
            "{budget:?} must outlast the exposure"
        );
        assert!(
            budget <= Duration::from_secs(9),
            "{budget:?} is longer than it needs to be"
        );
    }

    #[test]
    fn a_fast_shutter_does_not_buy_a_long_wait() {
        let budget = budget_for(&settings(Some((1.0f32 / 200.0).log2())));
        assert!(budget < Duration::from_secs(3));
    }

    #[test]
    fn bulb_falls_back_rather_than_inventing_a_number() {
        // A sentinel carries no stops, and its exposure ends when someone lets go.
        assert_eq!(budget_for(&settings(None)), UNKNOWN_SHUTTER_BUDGET);
    }

    #[test]
    fn an_absurd_shutter_falls_back_too() {
        // Two hours is not an exposure, it is broken arithmetic upstream.
        assert_eq!(
            budget_for(&settings(Some(7200f32.log2()))),
            UNKNOWN_SHUTTER_BUDGET
        );
    }

    #[test]
    fn no_shutter_reading_falls_back() {
        let exposure = ExposureSettings {
            shutter: None,
            aperture: None,
            iso: None,
        };
        assert_eq!(budget_for(&exposure), UNKNOWN_SHUTTER_BUDGET);
    }

    #[tokio::test(start_paused = true)]
    async fn a_write_that_works_first_time_does_not_wait() {
        let calls = Cell::new(0);
        let result = while_busy(Duration::from_secs(10), || {
            calls.set(calls.get() + 1);
            async { Ok(()) }
        })
        .await;

        assert!(result.is_ok());
        assert_eq!(calls.get(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_busy_camera_is_asked_again_until_it_is_free() {
        let calls = Cell::new(0);
        let result = while_busy(Duration::from_secs(10), || {
            calls.set(calls.get() + 1);
            // Busy for the first three attempts, as a body would be while the shutter is open.
            let busy = calls.get() < 4;
            async move {
                if busy {
                    Err(CameraError::Busy)
                } else {
                    Ok(())
                }
            }
        })
        .await;

        assert!(
            result.is_ok(),
            "the write should land once the exposure ends"
        );
        assert_eq!(calls.get(), 4);
    }

    #[tokio::test(start_paused = true)]
    async fn a_camera_that_never_frees_up_gives_up() {
        let calls = Cell::new(0);
        let result = while_busy(Duration::from_secs(2), || {
            calls.set(calls.get() + 1);
            async { Err(CameraError::Busy) }
        })
        .await;

        assert!(matches!(result, Err(CameraError::Busy)));
        // Bounded by the budget rather than spinning: ~2s at 300ms between tries.
        assert!((5..=8).contains(&calls.get()), "{} attempts", calls.get());
    }

    #[tokio::test(start_paused = true)]
    async fn no_budget_still_means_one_attempt() {
        let calls = Cell::new(0);
        let result = while_busy(Duration::ZERO, || {
            calls.set(calls.get() + 1);
            async { Err(CameraError::Busy) }
        })
        .await;

        assert!(matches!(result, Err(CameraError::Busy)));
        assert_eq!(calls.get(), 1);
    }

    /// Anything that is not busyness belongs to the caller immediately - retrying a value the
    /// body will never accept would just delay the message by the whole budget.
    #[tokio::test(start_paused = true)]
    async fn a_real_refusal_is_not_retried() {
        let calls = Cell::new(0);
        let result = while_busy(Duration::from_secs(10), || {
            calls.set(calls.get() + 1);
            async {
                Err(CameraError::Rejected {
                    status: 0x2002,
                    message: "no".into(),
                })
            }
        })
        .await;

        assert!(matches!(result, Err(CameraError::Rejected { .. })));
        assert_eq!(calls.get(), 1);
    }
}
