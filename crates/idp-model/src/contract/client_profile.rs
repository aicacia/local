use core::fmt;

#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(
    feature = "wasm",
    derive(tsify::Tsify),
    tsify(into_wasm_abi, from_wasm_abi)
)]
#[serde(rename_all = "snake_case")]
pub enum ClientProfile {
    /// Web applications that run on a server and interact with users via a web browser.
    #[serde(rename = "web_application")]
    Web = 0,
    /// Applications that run in a user's browser and interact with the user directly.
    #[serde(rename = "user_agent_based_application")]
    UserAgentBased = 1,
    /// Native applications that run on a user's device.
    #[serde(rename = "native_application")]
    Native = 2,
}

impl fmt::Display for ClientProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientProfile::Web => write!(f, "web_application"),
            ClientProfile::UserAgentBased => write!(f, "user_agent_based_application"),
            ClientProfile::Native => write!(f, "native_application"),
        }
    }
}
