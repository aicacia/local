#[cfg(not(feature = "std"))]
use alloc::string::String;

pub struct BootstrapInput {
    pub device_name: String,
    pub admin_username: String,
    pub admin_password: String,
}
