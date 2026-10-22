//! Three local buffer profiles. These names are not measured acoustic delays.
use cap_core::streaming::LocalBufferPolicy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaybackMode {
    UltraLow,
    #[default]
    Balanced,
    Stable,
}
impl PlaybackMode {
    pub fn policy(self) -> LocalBufferPolicy {
        match self {
            Self::UltraLow => LocalBufferPolicy::LowLatency,
            Self::Balanced => LocalBufferPolicy::Balanced,
            Self::Stable => LocalBufferPolicy::Stable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_are_distinct_and_do_not_evict_samples() {
        let profiles = [
            PlaybackMode::UltraLow,
            PlaybackMode::Balanced,
            PlaybackMode::Stable,
        ];
        let mut previous = 0;
        for mode in profiles {
            let options = mode.policy().options();
            assert!(options.startup_ms > previous);
            assert!(options.startup_ms <= options.buffer_ms);
            assert!(options.resume_ms <= options.refill_ms);
            assert!(options.refill_ms <= options.buffer_ms);
            assert!(options.max_age.is_none());
            previous = options.startup_ms;
            assert_eq!(
                serde_json::from_value::<PlaybackMode>(serde_json::to_value(mode).unwrap())
                    .unwrap(),
                mode
            );
        }
        assert!(serde_json::from_str::<PlaybackMode>("\"invalid\"").is_err());
        assert_eq!(PlaybackMode::default(), PlaybackMode::Balanced);
    }
}
