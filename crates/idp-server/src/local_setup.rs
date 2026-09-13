use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub use idp_model::contract::SetupStage;
use idp_service::generate_random_string;
use serde::{Deserialize, Serialize};

const STATE_FILE_NAME: &str = "setup-state.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LocalSetupState {
    pub device_identity_id: String,
    #[serde(default)]
    pub join: Option<LocalSetupJoin>,
    pub stage: SetupStage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LocalSetupJoin {
    pub device_name: String,
    pub endpoint_addr: String,
    #[serde(default)]
    pub joining_public_key: String,
    #[serde(default)]
    pub nonce: String,
}

pub struct LocalSetup {
    data_dir: PathBuf,
    state: Mutex<LocalSetupState>,
    token: Mutex<Option<String>>,
}

impl LocalSetup {
    pub fn new(data_dir: impl Into<PathBuf>, state: LocalSetupState) -> Self {
        let token = (!matches!(state.stage, SetupStage::Ready)).then(generate_random_string::<32>);
        Self {
            data_dir: data_dir.into(),
            state: Mutex::new(state),
            token: Mutex::new(token),
        }
    }

    pub fn stage(&self) -> SetupStage {
        self.state.lock().expect("setup state lock poisoned").stage
    }

    pub fn token(&self) -> Option<String> {
        self.token
            .lock()
            .expect("setup token lock poisoned")
            .clone()
    }

    pub fn authorize(&self, token: Option<&str>) -> bool {
        self.token
            .lock()
            .expect("setup token lock poisoned")
            .as_deref()
            == token
    }

    pub fn advance(&self, stage: SetupStage) -> io::Result<()> {
        let mut state = self.state.lock().expect("setup state lock poisoned");
        let valid_transition = state.stage == stage
            || matches!(
                (state.stage, stage),
                (SetupStage::Installation, SetupStage::Device)
                    | (SetupStage::Device, SetupStage::Ready)
            );
        if !valid_transition {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid setup stage transition",
            ));
        }
        state.stage = stage;
        state.save(&self.data_dir)?;
        if matches!(stage, SetupStage::Ready) {
            *self.token.lock().expect("setup token lock poisoned") = None;
        }
        Ok(())
    }

    pub fn join(&self) -> Option<LocalSetupJoin> {
        self.state
            .lock()
            .expect("setup state lock poisoned")
            .join
            .clone()
    }

    pub fn save_join(&self, join: LocalSetupJoin) -> io::Result<()> {
        let mut state = self.state.lock().expect("setup state lock poisoned");
        state.join = Some(join);
        state.save(&self.data_dir)
    }
}

impl LocalSetupState {
    pub fn load_or_create(data_dir: impl AsRef<Path>) -> io::Result<Self> {
        let data_dir = data_dir.as_ref();
        fs::create_dir_all(data_dir)?;
        let path = Self::path(data_dir);
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let state = Self {
                    device_identity_id: generate_random_string::<32>(),
                    join: None,
                    stage: SetupStage::Installation,
                };
                state.save(data_dir)?;
                Ok(state)
            }
            Err(error) => Err(error),
        }
    }

    pub fn path(data_dir: impl AsRef<Path>) -> PathBuf {
        data_dir.as_ref().join(STATE_FILE_NAME)
    }

    pub fn save(&self, data_dir: impl AsRef<Path>) -> io::Result<()> {
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        fs::write(Self::path(data_dir), bytes)
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::{LocalSetup, LocalSetupJoin, LocalSetupState};
    use idp_model::contract::SetupStage;

    #[test]
    fn serializes_setup_state() {
        let state = LocalSetupState {
            device_identity_id: "identity".to_owned(),
            join: Some(LocalSetupJoin {
                device_name: "device".to_owned(),
                endpoint_addr: "endpoint".to_owned(),
                joining_public_key: "public-key".to_owned(),
                nonce: "nonce".to_owned(),
            }),
            stage: SetupStage::Installation,
        };
        assert_eq!(
            serde_json::to_string(&state).expect("state serializes"),
            r#"{"device_identity_id":"identity","join":{"device_name":"device","endpoint_addr":"endpoint","joining_public_key":"public-key","nonce":"nonce"},"stage":"installation"}"#
        );
    }

    #[test]
    fn discards_token_when_setup_is_ready() {
        let data_dir =
            env::temp_dir().join(format!("idp-server-local-setup-{}", std::process::id()));
        fs::create_dir_all(&data_dir).expect("creates test data directory");
        let setup = LocalSetup::new(
            &data_dir,
            LocalSetupState {
                device_identity_id: "identity".to_owned(),
                join: None,
                stage: SetupStage::Installation,
            },
        );
        let token = setup.token().expect("creates setup token");

        assert!(setup.authorize(Some(&token)));
        setup
            .advance(SetupStage::Device)
            .expect("advances to device setup");
        setup
            .advance(SetupStage::Ready)
            .expect("persists ready stage");
        assert_eq!(setup.token(), None);
        assert!(!setup.authorize(Some(&token)));

        fs::remove_dir_all(data_dir).expect("removes test data directory");
    }

    #[test]
    fn persists_join_without_setup_token() {
        let data_dir = env::temp_dir().join(format!(
            "idp-server-local-setup-join-{}",
            std::process::id()
        ));
        fs::create_dir_all(&data_dir).expect("creates test data directory");
        let setup = LocalSetup::new(
            &data_dir,
            LocalSetupState {
                device_identity_id: "identity".to_owned(),
                join: None,
                stage: SetupStage::Installation,
            },
        );

        setup
            .save_join(LocalSetupJoin {
                device_name: "device".to_owned(),
                endpoint_addr: "endpoint".to_owned(),
                joining_public_key: "public-key".to_owned(),
                nonce: "nonce".to_owned(),
            })
            .expect("persists join");

        let saved = fs::read_to_string(LocalSetupState::path(&data_dir)).expect("reads state");
        assert!(saved.contains("\"join\""));
        assert!(!saved.contains("token"));

        fs::remove_dir_all(data_dir).expect("removes test data directory");
    }

    #[test]
    fn only_allows_forward_setup_transitions() {
        let data_dir = env::temp_dir().join(format!(
            "idp-server-local-setup-transitions-{}",
            std::process::id()
        ));
        fs::create_dir_all(&data_dir).expect("creates test data directory");
        let setup = LocalSetup::new(
            &data_dir,
            LocalSetupState {
                device_identity_id: "identity".to_owned(),
                join: None,
                stage: SetupStage::Installation,
            },
        );

        setup
            .advance(SetupStage::Device)
            .expect("join advances to device setup");
        setup
            .advance(SetupStage::Ready)
            .expect("device setup advances to ready");
        assert_eq!(
            setup
                .advance(SetupStage::Device)
                .expect_err("ready cannot return to device setup")
                .kind(),
            std::io::ErrorKind::InvalidInput
        );

        fs::remove_dir_all(data_dir).expect("removes test data directory");
    }
}
