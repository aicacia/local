use std::{collections::BTreeSet, sync::Arc};

use crate::{
    PasswordConfig, encrypt_password,
    repo::{KeyService, PrivateKeyRepo, RepoError, RepoResult, UserRepo},
};
use chrono::Utc;
use file_system::{FileSystem, PeerCodec, Storage, Transport};
use idp_model::{
    contract::EntityType,
    model::{User, UserEmail, UserPassword, UserPhoneNumber},
};

use super::{FsKeyRepo, JsonStore};

const USERS_FOLDER: &str = "idp/users";

pub struct FsUserRepo<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>, P> {
    store: JsonStore<S, C, T>,
    key_service: Arc<KeyService<FsKeyRepo<S, C, T>, P>>,
    password_config: PasswordConfig,
}

impl<S: Storage, C: PeerCodec, T: Transport<PeerId = C::PeerId>, P> Clone
    for FsUserRepo<S, C, T, P>
{
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            key_service: Arc::clone(&self.key_service),
            password_config: self.password_config.clone(),
        }
    }
}

impl<S, C, T, P> FsUserRepo<S, C, T, P>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
    P: PrivateKeyRepo + Send + Sync,
{
    #[must_use]
    pub fn new(
        file_system: Arc<FileSystem<S, C, T>>,
        key_service: Arc<KeyService<FsKeyRepo<S, C, T>, P>>,
        password_config: PasswordConfig,
    ) -> Self {
        Self {
            store: JsonStore::new(file_system),
            key_service,
            password_config,
        }
    }

    async fn users(&self) -> RepoResult<Vec<User>> {
        let paths = self.store.list(USERS_FOLDER).await?;
        let mut users: Vec<User> = Vec::with_capacity(paths.len());
        for path in paths {
            if path.ends_with(".json") {
                users.push(self.store.read(&path).await?);
            }
        }
        users.sort_by_key(|user| user.created_at);
        Ok(users)
    }

    async fn next_id(&self) -> RepoResult<i64> {
        // ponytail: scan records; replace with a replicated ID allocator only if user creation is hot.
        let ids = self
            .users()
            .await?
            .into_iter()
            .map(|user| user.id)
            .collect::<BTreeSet<_>>();
        random_id(&ids)
    }

    async fn require_user(&self, user_id: i64) -> RepoResult<()> {
        if self.find_user_by_id(user_id).await?.is_some() {
            Ok(())
        } else {
            Err(RepoError::InvalidInput("user was not found".into()))
        }
    }
}

impl<S, C, T, P> UserRepo for FsUserRepo<S, C, T, P>
where
    S: Storage + Send + Sync + 'static,
    S::Error: std::fmt::Display + Send + 'static,
    C: PeerCodec + Send + 'static,
    C::Error: Send + 'static,
    C::PeerId: Send + Sync + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Incoming: 'static,
    P: PrivateKeyRepo + Send + Sync,
{
    async fn find_user_by_id(&self, user_id: i64) -> RepoResult<Option<User>> {
        match self.store.read(&user_path(user_id)).await {
            Ok(user) => Ok(Some(user)),
            Err(RepoError::InvalidInput(message)) if message.contains("not found") => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn list_users(&self, offset: u32, limit: u32) -> RepoResult<Vec<User>> {
        Ok(self
            .users()
            .await?
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .collect())
    }

    async fn find_user_by_username_or_email(&self, identifier: &str) -> RepoResult<Option<User>> {
        let identifier = identifier.to_lowercase();
        for user in self.users().await? {
            if user.name.to_lowercase() == identifier
                || self
                    .find_user_emails_by_user_id(user.id)
                    .await?
                    .iter()
                    .any(|email| email.email.to_lowercase() == identifier)
            {
                return Ok(Some(user));
            }
        }
        Ok(None)
    }

    async fn find_user_emails_by_user_id(&self, user_id: i64) -> RepoResult<Vec<UserEmail>> {
        match self.store.read(&email_path(user_id)).await {
            Ok(email) => Ok(vec![email]),
            Err(RepoError::InvalidInput(message)) if message.contains("not found") => {
                Ok(Vec::new())
            }
            Err(error) => Err(error),
        }
    }

    async fn find_user_phone_numbers_by_user_id(
        &self,
        user_id: i64,
    ) -> RepoResult<Vec<UserPhoneNumber>> {
        match self.store.read(&phone_path(user_id)).await {
            Ok(phone) => Ok(vec![phone]),
            Err(RepoError::InvalidInput(message)) if message.contains("not found") => {
                Ok(Vec::new())
            }
            Err(error) => Err(error),
        }
    }

    async fn find_user_password_by_user_id(
        &self,
        user_id: i64,
    ) -> RepoResult<Option<UserPassword>> {
        match self
            .store
            .read::<UserPassword>(&password_path(user_id))
            .await
        {
            Ok(password) if password.active => Ok(Some(password)),
            Ok(_) => Ok(None),
            Err(RepoError::InvalidInput(message)) if message.contains("not found") => Ok(None),
            Err(error) => Err(error),
        }
    }

    async fn create_user_with_password(&self, name: &str, password: &str) -> RepoResult<User> {
        if password.trim().is_empty() {
            return Err(RepoError::InvalidInput(
                "password is required for user key material".into(),
            ));
        }
        if self.find_user_by_username_or_email(name).await?.is_some() {
            return Err(RepoError::InvalidInput("user name already exists".into()));
        }
        let now = Utc::now();
        let user = User {
            id: self.next_id().await?,
            name: name.into(),
            given_name: None,
            family_name: None,
            middle_name: None,
            nickname: None,
            profile: None,
            picture: None,
            website: None,
            sex: None,
            birthdate: None,
            zoneinfo: None,
            locale: None,
            created_at: now,
            updated_at: now,
        };
        self.store.write(&user_path(user.id), &user).await?;
        self.replace_user_password(user.id, password).await?;
        self.key_service
            .ensure_entity_master_key(EntityType::User, user.id, password)?;
        self.key_service
            .create_key(
                None,
                EntityType::User,
                user.id,
                true,
                user.name.clone(),
                None,
            )
            .await?;
        Ok(user)
    }

    async fn update_user(&self, mut user: User) -> RepoResult<User> {
        self.require_user(user.id).await?;
        user.updated_at = Utc::now();
        self.store.write(&user_path(user.id), &user).await?;
        Ok(user)
    }

    async fn upsert_primary_user_email(
        &self,
        user_id: i64,
        email: &str,
        verified: bool,
    ) -> RepoResult<()> {
        self.require_user(user_id).await?;
        let now = Utc::now();
        let existing = self
            .store
            .read::<UserEmail>(&email_path(user_id))
            .await
            .ok();
        self.store
            .write(
                &email_path(user_id),
                &UserEmail {
                    id: existing
                        .as_ref()
                        .map_or_else(|| random_id(&BTreeSet::new()), |email| Ok(email.id))?,
                    user_id,
                    email: email.into(),
                    verified,
                    primary: true,
                    created_at: existing.as_ref().map_or(now, |email| email.created_at),
                    updated_at: now,
                },
            )
            .await
    }

    async fn upsert_primary_user_phone_number(
        &self,
        user_id: i64,
        phone_number: &str,
        verified: bool,
    ) -> RepoResult<()> {
        self.require_user(user_id).await?;
        let now = Utc::now();
        let existing = self
            .store
            .read::<UserPhoneNumber>(&phone_path(user_id))
            .await
            .ok();
        self.store
            .write(
                &phone_path(user_id),
                &UserPhoneNumber {
                    id: existing
                        .as_ref()
                        .map_or_else(|| random_id(&BTreeSet::new()), |phone| Ok(phone.id))?,
                    user_id,
                    phone_number: phone_number.into(),
                    verified,
                    primary: true,
                    created_at: existing.as_ref().map_or(now, |phone| phone.created_at),
                    updated_at: now,
                },
            )
            .await
    }

    async fn replace_user_password(&self, user_id: i64, password: &str) -> RepoResult<()> {
        self.require_user(user_id).await?;
        let now = Utc::now();
        self.store
            .write(
                &password_path(user_id),
                &UserPassword {
                    id: random_id(&BTreeSet::new())?,
                    user_id,
                    active: true,
                    password_hash: encrypt_password(&self.password_config, password)
                        .map_err(RepoError::other)?,
                    created_at: now,
                    updated_at: now,
                },
            )
            .await
    }

    async fn delete_user_by_id(&self, user_id: i64) -> RepoResult<()> {
        self.require_user(user_id).await?;
        self.store.delete(&user_path(user_id)).await?;
        let _ = self.store.delete(&email_path(user_id)).await;
        let _ = self.store.delete(&phone_path(user_id)).await;
        let _ = self.store.delete(&password_path(user_id)).await;
        Ok(())
    }
}

fn random_id(ids: &BTreeSet<i64>) -> RepoResult<i64> {
    loop {
        let mut bytes = [0_u8; 8];
        getrandom::fill(&mut bytes).map_err(RepoError::other)?;
        let id = i64::from_be_bytes(bytes) & i64::MAX;
        if id != 0 && !ids.contains(&id) {
            return Ok(id);
        }
    }
}

fn user_path(user_id: i64) -> String {
    format!("{USERS_FOLDER}/{user_id}.json")
}

fn email_path(user_id: i64) -> String {
    format!("idp/user-emails/{user_id}.json")
}

fn phone_path(user_id: i64) -> String {
    format!("idp/user-phones/{user_id}.json")
}

fn password_path(user_id: i64) -> String {
    format!("idp/user-passwords/{user_id}.json")
}
