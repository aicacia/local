use core::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum DeviceState {
    Pending = 0,
    Approved = 1,
    Revoked = 2,
}

impl fmt::Display for DeviceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Approved => write!(f, "approved"),
            Self::Revoked => write!(f, "revoked"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DeviceState;

    #[test]
    fn serializes_as_a_stable_name() {
        assert_eq!(
            serde_json::to_string(&DeviceState::Approved).unwrap(),
            "\"approved\""
        );
    }
}
