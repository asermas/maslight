//! Connecting the local API to the application state.
//!
//! The API crate knows nothing about Tauri, and the application knows nothing
//! about HTTP. This is the one file that knows both, which is what keeps the
//! API optional: nothing else changes when it is turned off.

use std::sync::Arc;

use maslight_api::{ApiBackend, ApiServer};
use maslight_core::{AppConfig, Rgb8};
use maslight_engine::EngineStatus;

use crate::Shared;

/// Exposes the shared state through the API trait.
pub struct Bridge {
    shared: Arc<Shared>,
}

impl Bridge {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
    }
}

impl ApiBackend for Bridge {
    fn status(&self) -> EngineStatus {
        self.shared.engine.lock().status()
    }

    fn config(&self) -> AppConfig {
        self.shared.config.lock().clone()
    }

    fn apply(&self, config: AppConfig) -> Result<(), String> {
        self.shared.commit(config)
    }

    fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let mut config = self.shared.config.lock().clone();
        config.enabled = enabled;
        self.shared.commit(config)
    }

    fn set_profile(&self, id: &str) -> Result<(), String> {
        let mut config = self.shared.config.lock().clone();
        if config.profile(id).is_none() {
            return Err(format!("no profile with id {id}"));
        }
        config.active_profile = id.to_string();
        self.shared.commit(config)
    }

    fn hold(&self, color: Option<Rgb8>) {
        self.shared.engine.lock().hold_color(color);
    }

    fn identify(&self, index: usize, ms: u64) {
        self.shared.engine.lock().identify(index, ms);
    }
}

/// Start or stop the server so it matches the configuration.
///
/// Called after every commit, so turning the switch on in Settings is all it
/// takes. Returns the running port, if any.
pub fn reconcile(shared: &Arc<Shared>, server: &mut Option<ApiServer>) -> Option<u16> {
    let (wanted, port, token) = {
        let config = shared.config.lock();
        (
            config.ui.enable_api,
            config.ui.api_port,
            config.ui.api_token.clone(),
        )
    };

    match (wanted, server.as_ref()) {
        (false, Some(_)) => {
            *server = None;
            tracing::info!("local API stopped");
            None
        }
        (false, None) => None,
        (true, Some(running)) if running.port() == port => Some(port),
        (true, _) => {
            // A port change means a restart, which is one line rather than a
            // special case.
            *server = None;
            match ApiServer::start(Arc::new(Bridge::new(Arc::clone(shared))), port, token) {
                Ok(started) => {
                    let port = started.port();
                    *server = Some(started);
                    Some(port)
                }
                Err(e) => {
                    tracing::warn!("local API could not start: {e}");
                    None
                }
            }
        }
    }
}
