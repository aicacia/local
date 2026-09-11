#[cfg(not(feature = "std"))]
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    vec::Vec,
};

use chrono::{DateTime, Utc};

use crate::contract::{Sex, UserInfo};

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: i64,

    pub name: String,

    pub given_name: Option<String>,

    pub family_name: Option<String>,

    pub middle_name: Option<String>,

    pub nickname: Option<String>,

    pub profile: Option<String>,

    pub picture: Option<String>,

    pub website: Option<String>,

    #[serde(with = "super::sql_enum::sex_option")]
    pub sex: Option<Sex>,

    pub birthdate: Option<DateTime<Utc>>,

    pub zoneinfo: Option<String>,

    pub locale: Option<String>,

    #[serde(with = "chrono::serde::ts_seconds")]
    pub created_at: DateTime<Utc>,
    #[serde(with = "chrono::serde::ts_seconds")]
    pub updated_at: DateTime<Utc>,
}

impl From<User> for UserInfo {
    fn from(val: User) -> Self {
        UserInfo {
            sub: val.id.to_string(),
            permissions: Vec::new(),
            name: Some(val.name),
            given_name: val.given_name,
            family_name: val.family_name,
            middle_name: val.middle_name,
            nickname: val.nickname.clone(),
            preferred_username: val.nickname,
            profile: val.profile,
            picture: val.picture,
            website: val.website,
            // Email must be added after converting to UserInfo,
            // because the email is not stored in the User model, but in the UserEmail model.
            email: None,
            email_verified: None,
            // Phone number must be added after converting to UserInfo,
            // because the phone number is not stored in the User model, but in the UserPhone model.
            phone_number: None,
            phone_number_verified: None,
            gender: val.sex.map(|s| s.to_string()),
            birthdate: val.birthdate.map(|s| s.to_rfc3339()),
            zoneinfo: val.zoneinfo,
            locale: val.locale,
            updated_at: DateTime::from_timestamp_secs(val.updated_at.timestamp())
                .unwrap_or_default(),
        }
    }
}
