use std::{
    collections::BTreeMap,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use lidp_model::contract::StorageSession;

const TOKEN_BYTES: usize = 32;
const TOKEN_TTL: Duration = Duration::from_secs(60);

#[derive(Clone, Eq, PartialEq)]
pub struct StorageScope {
    pub user_sub: String,
    pub client_id: String,
    pub access_token: String,
}

impl std::fmt::Debug for StorageScope {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StorageScope")
            .field("user_sub", &self.user_sub)
            .field("client_id", &self.client_id)
            .field("access_token", &"[redacted]")
            .finish()
    }
}

pub struct StorageSessionService {
    sessions: Mutex<BTreeMap<String, (StorageScope, i64)>>,
}

impl Default for StorageSessionService {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageSessionService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn issue(&self, scope: StorageScope) -> Result<StorageSession, getrandom::Error> {
        let expires_at = now().saturating_add(TOKEN_TTL.as_secs() as i64);
        let mut bytes = [0_u8; TOKEN_BYTES];
        getrandom::fill(&mut bytes)?;
        let token = URL_SAFE_NO_PAD.encode(bytes);
        let mut sessions = self
            .sessions
            .lock()
            .expect("storage sessions lock poisoned");
        sessions.retain(|_, (_, expiration)| *expiration > now());
        sessions.insert(token.clone(), (scope, expires_at));
        Ok(StorageSession { token, expires_at })
    }

    pub fn take(&self, token: &str) -> Option<StorageScope> {
        let (scope, expires_at) = self
            .sessions
            .lock()
            .expect("storage sessions lock poisoned")
            .remove(token)?;
        (expires_at > now()).then_some(scope)
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before Unix epoch")
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::{StorageScope, StorageSessionService};

    #[test]
    fn consumes_storage_sessions_once() {
        let sessions = StorageSessionService::new();
        let issued = sessions
            .issue(StorageScope {
                user_sub: "user".into(),
                client_id: "client".into(),
                access_token: "token".into(),
            })
            .unwrap();

        assert_eq!(
            sessions.take(&issued.token),
            Some(StorageScope {
                user_sub: "user".into(),
                client_id: "client".into(),
                access_token: "token".into(),
            })
        );
        assert_eq!(sessions.take(&issued.token), None);
    }

    #[test]
    fn redacts_access_tokens_from_debug_output() {
        let scope = StorageScope {
            user_sub: "user".into(),
            client_id: "client".into(),
            access_token: "secret".into(),
        };
        assert!(!format!("{scope:?}").contains("secret"));
    }
}
