//! Settings that shape a session rather than an exposure.
//!
//! Deliberately a grab-bag: these are the secondary controls, the ones set once when a run starts
//! and then left alone. The ramp has its own module because its values are read by the planner on
//! every frame; nothing here participates in a decision.
//!
//! Owned by Rust for the same reason the ramp is: a WebView reload must not silently undo a choice
//! made an hour into a sequence. Losing "transfer one frame in five" would quietly start pulling
//! five times the data over a link that was thinned on purpose.

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Fewest and most frames one transfer may stand for.
///
/// 1 is every frame. 30 is well past useful and only exists so the value has an end - at a two
/// second interval it is still a measurement a minute.
pub const TRANSFER_EVERY_MIN: u32 = 1;
pub const TRANSFER_EVERY_MAX: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    /// Transfer one frame in this many. 1 is every frame.
    ///
    /// Frames that are not transferred are still counted and timed - they simply never cross the
    /// network, so nothing measures them. That is the point: on a short interval the sequence does
    /// not need a reading from every frame, and the radio is what drains the battery.
    pub transfer_every: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { transfer_every: 1 }
    }
}

impl AppSettings {
    /// The same settings with anything out of range brought back into it.
    fn clamped(self) -> Self {
        Self {
            transfer_every: self
                .transfer_every
                .clamp(TRANSFER_EVERY_MIN, TRANSFER_EVERY_MAX),
        }
    }
}

/// The stored settings.
#[derive(Default)]
pub struct SettingsState(RwLock<AppSettings>);

impl SettingsState {
    pub async fn get(&self) -> AppSettings {
        *self.0.read().await
    }

    /// Store new settings and return what was actually kept.
    ///
    /// Clamped here rather than trusted: a zero would make the frontend's "every nth frame"
    /// arithmetic divide by it, and the bounds belong with the value rather than with whichever
    /// control happens to be editing it.
    pub async fn set(&self, settings: AppSettings) -> AppSettings {
        let mut guard = self.0.write().await;
        *guard = settings.clamped();
        *guard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfers_every_frame_by_default() {
        // Anything else would quietly discard frames nobody asked to discard.
        assert_eq!(AppSettings::default().transfer_every, 1);
    }

    #[tokio::test]
    async fn a_zero_is_pulled_up_to_one() {
        let state = SettingsState::default();
        // Zero would be a division by zero in the caller that picks which frame to fetch.
        let stored = state.set(AppSettings { transfer_every: 0 }).await;
        assert_eq!(stored.transfer_every, 1);
    }

    #[tokio::test]
    async fn an_absurd_value_is_capped() {
        let state = SettingsState::default();
        let stored = state.set(AppSettings { transfer_every: 5000 }).await;
        assert_eq!(stored.transfer_every, TRANSFER_EVERY_MAX);
    }

    #[tokio::test]
    async fn settings_survive_being_read_back() {
        let state = SettingsState::default();
        state.set(AppSettings { transfer_every: 4 }).await;
        assert_eq!(state.get().await.transfer_every, 4);
    }
}
