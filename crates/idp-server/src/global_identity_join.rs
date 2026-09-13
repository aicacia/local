use std::{collections::BTreeMap, sync::Arc};

use file_system::{PeerCodec, Transport};
use idp_model::contract::{
    GlobalIdentityJoinOffer, GlobalIdentityJoinReply, GlobalIdentityRow, GlobalIdentityTable,
    GlobalIdentityValue,
};
use storage_service::GlobalIdentityRuntime;

use crate::{GlobalBootstrapGrants, GlobalIdentityRevisionWriter};

pub struct GlobalIdentityJoinApprover<C, T>
where
    C: PeerCodec,
    T: Transport<PeerId = C::PeerId>,
{
    writer: GlobalIdentityRevisionWriter<C, T>,
    grants: Arc<GlobalBootstrapGrants>,
    accepting_public_key: String,
    accepting_endpoint_addr: String,
}

impl<C, T> GlobalIdentityJoinApprover<C, T>
where
    C: PeerCodec + Send + Sync + 'static,
    C::Error: std::fmt::Display + Send + 'static,
    C::PeerId: Send + 'static,
    T: Transport<PeerId = C::PeerId> + Send + Sync + 'static,
    T::Error: std::fmt::Display,
    T::Incoming: Send + 'static,
{
    #[must_use]
    pub fn new(
        runtime: Arc<GlobalIdentityRuntime<C, T>>,
        cache: crate::GlobalIdentityCache,
        grants: Arc<GlobalBootstrapGrants>,
        accepting_public_key: String,
        accepting_endpoint_addr: String,
    ) -> Self {
        Self {
            writer: GlobalIdentityRevisionWriter::new(runtime, cache),
            grants,
            accepting_public_key,
            accepting_endpoint_addr,
        }
    }

    pub async fn approve(
        &self,
        remote_public_key: &str,
        offer: GlobalIdentityJoinOffer,
        revision: String,
        expires_at: i64,
    ) -> Result<GlobalIdentityJoinReply, String> {
        if !offer.is_valid() || offer.joining_public_key != remote_public_key {
            return Err("invalid global identity join offer".to_owned());
        }
        let accepting_public_key = self.accepting_public_key.clone();
        let joining_public_key = offer.joining_public_key.clone();
        let nonce = offer.nonce.clone();
        let manifest = self
            .writer
            .apply(revision, move |rows| {
                if rows.iter().any(|row| {
                    row.table == GlobalIdentityTable::Devices
                        && row.columns.get("public_key")
                            == Some(&GlobalIdentityValue::Text(joining_public_key.clone()))
                }) {
                    return Err("joining device already exists".to_owned());
                }
                rows.push(approved_device(
                    rows.iter().map(|row| row.id).max().unwrap_or(0) + 1,
                    offer,
                    accepting_public_key,
                ));
                Ok(())
            })
            .await?;
        let grant = self.grants.issue(
            remote_public_key.to_owned(),
            self.accepting_public_key.clone(),
            nonce,
            expires_at,
        );
        Ok(GlobalIdentityJoinReply {
            accepting_endpoint_addr: self.accepting_endpoint_addr.clone(),
            target_revision: manifest.revision,
            grant,
        })
    }
}

fn approved_device(
    id: i64,
    offer: GlobalIdentityJoinOffer,
    accepting_public_key: String,
) -> GlobalIdentityRow {
    let mut columns = BTreeMap::new();
    columns.insert(
        "name".to_owned(),
        GlobalIdentityValue::Text(offer.device_name),
    );
    columns.insert(
        "public_key".to_owned(),
        GlobalIdentityValue::Text(offer.joining_public_key),
    );
    columns.insert(
        "address".to_owned(),
        GlobalIdentityValue::Text(offer.joining_endpoint_addr),
    );
    columns.insert("enrollment_code_hash".to_owned(), GlobalIdentityValue::Null);
    columns.insert(
        "enrollment_expires_at".to_owned(),
        GlobalIdentityValue::Null,
    );
    columns.insert(
        "pairing_accepting_public_key".to_owned(),
        GlobalIdentityValue::Text(accepting_public_key),
    );
    columns.insert("state".to_owned(), GlobalIdentityValue::Integer(1));
    columns.insert("created_at".to_owned(), GlobalIdentityValue::Integer(0));
    columns.insert("updated_at".to_owned(), GlobalIdentityValue::Integer(0));
    columns.insert("revoked_at".to_owned(), GlobalIdentityValue::Null);
    GlobalIdentityRow {
        table: GlobalIdentityTable::Devices,
        id,
        columns,
    }
}

#[cfg(test)]
mod tests {
    use idp_model::contract::{GlobalIdentityJoinOffer, GlobalIdentityValue};

    use super::approved_device;

    #[test]
    fn approval_creates_an_active_device_bound_to_the_accepting_peer() {
        let row = approved_device(
            1,
            GlobalIdentityJoinOffer {
                device_name: "device".to_owned(),
                joining_endpoint_addr: "address".to_owned(),
                joining_public_key: "joining".to_owned(),
                nonce: "nonce".to_owned(),
            },
            "accepting".to_owned(),
        );

        assert!(row.is_valid());
        assert_eq!(
            row.columns.get("state"),
            Some(&GlobalIdentityValue::Integer(1))
        );
        assert_eq!(
            row.columns.get("pairing_accepting_public_key"),
            Some(&GlobalIdentityValue::Text("accepting".to_owned()))
        );
    }
}
