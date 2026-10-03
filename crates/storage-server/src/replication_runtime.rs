use std::{io, sync::Arc, time::Duration};

use iroh::EndpointId;
use iroh_chain::Server;
use storage_service::{DatabaseRuntime, ScopedFileSystemRuntime};
use tokio::{select, task::JoinHandle, time::interval};
use tokio_util::sync::CancellationToken;

use crate::{
    ManagementClient, data_protocol::DataProtocolHandler,
    database_protocol::DatabaseProtocolHandler, storage_protocol::StorageProtocolHandler,
};

pub struct StorageReplicationRuntime {
    data_handler: DataProtocolHandler,
    sync_task: JoinHandle<()>,
}

impl StorageReplicationRuntime {
    pub fn start(
        server: Server,
        management: ManagementClient,
        databases: Arc<DatabaseRuntime>,
        file_systems: Arc<ScopedFileSystemRuntime<EndpointId>>,
        cancellation_token: CancellationToken,
    ) -> io::Result<Self> {
        let database = DatabaseProtocolHandler::new(server.clone(), management.clone(), databases);
        let file_system = StorageProtocolHandler::new(server.clone(), management, file_systems);
        let data_handler = DataProtocolHandler::new(database.clone(), file_system.clone());
        let sync_task = tokio::spawn(async move {
            let mut sync_interval = interval(Duration::from_secs(10));
            loop {
                select! {
                    () = cancellation_token.cancelled() => return,
                    _ = sync_interval.tick() => {
                        database.synchronize_selected_peers().await;
                        file_system.synchronize_selected_peers().await;
                    }
                }
            }
        });
        Ok(Self {
            data_handler,
            sync_task,
        })
    }

    pub fn data_handler(&self) -> impl iroh::protocol::ProtocolHandler + Clone {
        self.data_handler.clone()
    }

    pub async fn shutdown(self) -> io::Result<()> {
        self.sync_task.abort();
        match self.sync_task.await {
            Ok(()) => {}
            Err(error) if error.is_cancelled() => {}
            Err(error) => return Err(io::Error::other(error)),
        }
        Ok(())
    }
}
