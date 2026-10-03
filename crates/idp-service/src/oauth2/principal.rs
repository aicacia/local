use core::any::Any;

use idp_model::{
    contract::EntityType,
    model::{Client, Id, Key, User},
};

pub trait Principal: Send + Sync {
    fn get_entity_id(&self) -> Id;
    fn get_entity_type(&self) -> EntityType;
    fn get_entity_as_any(&self) -> &dyn Any;
    fn get_key(&self) -> &Key;
}

pub struct UserPrincipal {
    pub user: User,
    pub key: Key,
}

pub struct ClientPrincipal {
    pub client: Client,
    pub key: Key,
}

impl Principal for UserPrincipal {
    fn get_entity_id(&self) -> Id {
        self.user.id
    }

    fn get_entity_type(&self) -> EntityType {
        EntityType::User
    }

    fn get_entity_as_any(&self) -> &dyn Any {
        &self.user
    }

    fn get_key(&self) -> &Key {
        &self.key
    }
}

impl Principal for ClientPrincipal {
    fn get_entity_id(&self) -> Id {
        self.client.id
    }

    fn get_entity_type(&self) -> EntityType {
        EntityType::Client
    }

    fn get_entity_as_any(&self) -> &dyn Any {
        &self.client
    }

    fn get_key(&self) -> &Key {
        &self.key
    }
}
