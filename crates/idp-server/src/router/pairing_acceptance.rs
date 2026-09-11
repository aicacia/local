use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use iroh_chain::{AllowedEndpointId, Server, TunnelAuthorizer};

pub trait PairingAcceptanceController: Send + Sync + 'static {
    fn set_pairing_accepting(&self, accepting: bool) -> Result<(), String>;
    fn pairing_accepting(&self) -> Result<bool, String>;
}

pub struct TimedPairingAcceptanceController<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    server: Server<A, V>,
    timeout: Duration,
}

impl<A, V> TimedPairingAcceptanceController<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    #[must_use]
    pub fn new(server: Server<A, V>, timeout: Duration) -> Self {
        Self { server, timeout }
    }
}

impl<A, V> PairingAcceptanceController for TimedPairingAcceptanceController<A, V>
where
    A: AllowedEndpointId,
    V: TunnelAuthorizer,
{
    fn set_pairing_accepting(&self, accepting: bool) -> Result<(), String> {
        if accepting {
            self.server.set_pairing_enabled_for(self.timeout);
        } else {
            self.server.set_pairing_enabled(false);
        }
        Ok(())
    }

    fn pairing_accepting(&self) -> Result<bool, String> {
        Ok(self.server.pairing_enabled())
    }
}

pub struct PairingAcceptanceControllerSlot {
    controller: Mutex<Option<Arc<dyn PairingAcceptanceController>>>,
}

impl PairingAcceptanceControllerSlot {
    #[must_use]
    pub fn new() -> Self {
        Self {
            controller: Mutex::new(None),
        }
    }

    pub fn bind(&self, controller: Arc<dyn PairingAcceptanceController>) -> Result<(), String> {
        let mut slot = self
            .controller
            .lock()
            .map_err(|_| "pairing controller lock is poisoned".to_owned())?;
        *slot = Some(controller);
        Ok(())
    }

    fn controller(&self) -> Result<Arc<dyn PairingAcceptanceController>, String> {
        self.controller
            .lock()
            .map_err(|_| "pairing controller lock is poisoned".to_owned())?
            .clone()
            .ok_or_else(|| "pairing controller is not initialized".to_owned())
    }
}

impl Default for PairingAcceptanceControllerSlot {
    fn default() -> Self {
        Self::new()
    }
}

impl PairingAcceptanceController for PairingAcceptanceControllerSlot {
    fn set_pairing_accepting(&self, accepting: bool) -> Result<(), String> {
        self.controller()?.set_pairing_accepting(accepting)
    }

    fn pairing_accepting(&self) -> Result<bool, String> {
        self.controller()?.pairing_accepting()
    }
}
