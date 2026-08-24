use std::fmt;

use serde::{Deserialize, Serialize};

/// The externally visible capacity state of a contributed node.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NodeState {
    Full,
    Throttled,
    Drained,
    Offline,
}

impl fmt::Display for NodeState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Full => "FULL",
            Self::Throttled => "THROTTLED",
            Self::Drained => "DRAINED",
            Self::Offline => "OFFLINE",
        };

        formatter.write_str(name)
    }
}

/// The human-selected operating mode for a contributed workstation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UserMode {
    Auto,
    Work,
    Gaming,
    Idle,
    Maintenance,
    ForceCi,
}

impl fmt::Display for UserMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Auto => "auto",
            Self::Work => "work",
            Self::Gaming => "gaming",
            Self::Idle => "idle",
            Self::Maintenance => "maintenance",
            Self::ForceCi => "force-ci",
        };

        formatter.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::{NodeState, UserMode};

    #[test]
    fn node_state_display_contract() {
        let cases = [
            (NodeState::Full, "FULL"),
            (NodeState::Throttled, "THROTTLED"),
            (NodeState::Drained, "DRAINED"),
            (NodeState::Offline, "OFFLINE"),
        ];

        for (state, expected) in cases {
            assert_eq!(state.to_string(), expected);
        }
    }

    #[test]
    fn node_state_json_contract() {
        let cases = [
            (NodeState::Full, "FULL"),
            (NodeState::Throttled, "THROTTLED"),
            (NodeState::Drained, "DRAINED"),
            (NodeState::Offline, "OFFLINE"),
        ];

        for (state, expected) in cases {
            let json = format!("\"{expected}\"");
            assert_eq!(serde_json::to_string(&state).unwrap(), json);
            assert_eq!(serde_json::from_str::<NodeState>(&json).unwrap(), state);
        }

        for invalid in ["full", "Full", "UNKNOWN"] {
            assert!(serde_json::from_str::<NodeState>(&format!("\"{invalid}\"")).is_err());
        }
    }

    #[test]
    fn user_mode_display_contract() {
        let cases = [
            (UserMode::Auto, "auto"),
            (UserMode::Work, "work"),
            (UserMode::Gaming, "gaming"),
            (UserMode::Idle, "idle"),
            (UserMode::Maintenance, "maintenance"),
            (UserMode::ForceCi, "force-ci"),
        ];

        for (mode, expected) in cases {
            assert_eq!(mode.to_string(), expected);
        }
    }

    #[test]
    fn user_mode_json_contract() {
        let cases = [
            (UserMode::Auto, "auto"),
            (UserMode::Work, "work"),
            (UserMode::Gaming, "gaming"),
            (UserMode::Idle, "idle"),
            (UserMode::Maintenance, "maintenance"),
            (UserMode::ForceCi, "force-ci"),
        ];

        for (mode, expected) in cases {
            let json = format!("\"{expected}\"");
            assert_eq!(serde_json::to_string(&mode).unwrap(), json);
            assert_eq!(serde_json::from_str::<UserMode>(&json).unwrap(), mode);
        }

        for invalid in ["force_ci", "ForceCi", "WORK", "unknown"] {
            assert!(serde_json::from_str::<UserMode>(&format!("\"{invalid}\"")).is_err());
        }
    }
}
