use std::{path::PathBuf, sync::Arc};

use axum::extract::ws::{Message, WebSocket};
use serde::{Deserialize, Serialize};
use serde_json::json;
use storage_iroh::{StorageEvent, StorageIrohRuntime, StorageRequest, StorageResponse};
use storage_service::StorageService;

#[derive(Debug, Clone)]
pub struct LocalStorage {
    runtime: StorageIrohRuntime,
    storage: Arc<StorageService>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
enum StorageMessage {
    Request {
        #[serde(flatten)]
        request: StorageRequest,
        #[serde(rename = "requestId")]
        request_id: u64,
    },
    Response {
        #[serde(flatten)]
        response: StorageResponse,
        #[serde(rename = "requestId")]
        request_id: u64,
    },
    Event(StorageEvent),
}

impl LocalStorage {
    pub async fn new(files_dir: PathBuf) -> Self {
        let _ = tokio::fs::create_dir_all(&files_dir).await;

        Self {
            runtime: StorageIrohRuntime::new(),
            storage: Arc::new(StorageService::new(files_dir)),
        }
    }

    async fn handle_storage_request(
        &self,
        request: StorageRequest,
    ) -> Result<StorageResponse, String> {
        let result = match request {
            StorageRequest::ReadFile { path } => {
                let bytes = self
                    .storage
                    .read_file(&path)
                    .await
                    .map_err(|e| e.to_string())?;
                let content = String::from_utf8(bytes).map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_payload(content))
            }
            StorageRequest::WriteFile { path, content } => {
                self.storage
                    .write_file(&path, content.as_bytes())
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_empty())
            }
            StorageRequest::ListDir { path } => {
                let entries = self
                    .storage
                    .list_dir(&path)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_payload(json!(entries)))
            }
            StorageRequest::CreateDir { path } => {
                self.storage
                    .create_dir(&path)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_empty())
            }
            StorageRequest::DeletePath { path } => {
                self.storage
                    .delete_path(&path)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_empty())
            }
            StorageRequest::RenamePath { from, to } => {
                self.storage
                    .rename_path(&from, &to)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_empty())
            }
            StorageRequest::ExistsPath { path } => {
                let exists = self
                    .storage
                    .exists(&path)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(StorageResponse::success_payload(json!(exists)))
            }
            StorageRequest::AddDevice { device_id } => Ok(StorageResponse::success_event(
                self.runtime.add_device(&device_id).await?,
            )),
            StorageRequest::RemoveDevice { device_id } => Ok(StorageResponse::success_event(
                self.runtime.remove_device(&device_id).await?,
            )),
            StorageRequest::ConnectPeer { peer_id } => Ok(StorageResponse::success_event(
                self.runtime.connect_peer(&peer_id).await?,
            )),
            StorageRequest::SyncPeer { peer_id } => Ok(StorageResponse::success_event(
                self.runtime.sync_peer(&peer_id).await?,
            )),
            StorageRequest::SendMessage { peer_id, payload } => Ok(StorageResponse::success_event(
                self.runtime.send_message(&peer_id, &payload).await?,
            )),
            StorageRequest::CloseSession { peer_id } => Ok(StorageResponse::success_event(
                self.runtime.close_session(&peer_id).await?,
            )),
        };

        result
    }

    pub async fn handle_socket(mut socket: WebSocket, storage: Arc<Self>) {
        let mut event_rx = storage.runtime.subscribe_events();

        loop {
            tokio::select! {
                msg = socket.recv() => {
                    let Some(msg) = msg else {
                        break;
                    };

                    let Ok(Message::Text(raw)) = msg else {
                        continue;
                    };


                    match serde_json::from_str::<StorageMessage>(&raw) {
                        Ok(StorageMessage::Request { request, request_id }) => {
                            let response = match storage.handle_storage_request(request).await {
                                Ok(r) => r,
                                Err(err) => StorageResponse::error(err),
                            };

                            let payload = match serde_json::to_string(&StorageMessage::Response {
                                response,
                                request_id,
                            }) {
                                Ok(payload) => payload,
                                Err(_) => {
                                    let fallback = "{\"ok\":false,\"error\":\"serialization failed\"}".to_string();
                                    if socket.send(Message::Text(fallback.into())).await.is_err() {
                                        break;
                                    }
                                    continue;
                                }
                            };

                            if socket.send(Message::Text(payload.into())).await.is_err() {
                                break;
                            }
                        }
                        Ok(StorageMessage::Response { .. }) | Ok(StorageMessage::Event(_)) => {}
                        Err(error) => {
                            log::error!("local storage rejected websocket frame: {error}");
                        }
                    }
                }
                Ok(event) = event_rx.recv() => {
                    let payload = match serde_json::to_string(&StorageMessage::Event(event)) {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    if socket.send(Message::Text(payload.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
}
