use std::{
    collections::HashMap,
    sync::{Arc, Mutex, RwLock},
};

use reqwest::Client;
use sqlx::PgPool;
use tokio::sync::broadcast;

use crate::{
    agents::{locks::KeyedLockStore, runs::AgentRunStore},
    callbacks::{litellm_db::LiteLLMDBCallback, CallbackManager},
    errors::GatewayError,
    mcp::registry::McpServerRegistry,
    model_prices::ModelCostMap,
    object_storage::ObjectStorageClient,
    proxy::{auth::api_keys::GatewayApiKeyStore, config::GatewayConfig},
    sdk::routing::Router,
};

#[derive(Debug)]
pub struct AppState {
    pub config: GatewayConfig,
    pub router: Router,
    pub mcp_servers: McpServerRegistry,
    pub http: Client,
    pub model_cost_map: ModelCostMap,
    pub agent_runs: AgentRunStore,
    pub keyed_locks: KeyedLockStore,
    pub db: Option<PgPool>,
    pub api_keys: GatewayApiKeyStore,
    pub callbacks: CallbackManager,
    pub object_storage: Option<ObjectStorageClient>,
    pub local_session_events: LocalSessionEvents,
    pub provider_consumers: ProviderConsumers,
    mcp_proxy_base_url: RwLock<Option<String>>,
}

/// Registry of per-session provider stream consumer tasks. Exactly one task
/// per session consumes the runtime provider's event stream (persisting and
/// publishing to LocalSessionEvents); any number of SSE subscribers share it
/// instead of each opening their own provider connection.
#[derive(Debug, Default)]
pub struct ProviderConsumers {
    tasks: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
}

impl ProviderConsumers {
    /// True when a live consumer task exists; prunes finished entries.
    pub fn is_running(&self, session_id: &str) -> bool {
        let mut tasks = self.tasks.lock().expect("provider consumers lock");
        match tasks.get(session_id) {
            Some(handle) if !handle.is_finished() => true,
            Some(_) => {
                tasks.remove(session_id);
                false
            }
            None => false,
        }
    }

    /// Registers a consumer spawned by `spawn` unless a live one already
    /// exists (lost race). Returns whether the new task was installed.
    pub fn install(
        &self,
        session_id: &str,
        spawn: impl FnOnce() -> tokio::task::JoinHandle<()>,
    ) -> bool {
        let mut tasks = self.tasks.lock().expect("provider consumers lock");
        if let Some(handle) = tasks.get(session_id) {
            if !handle.is_finished() {
                return false;
            }
        }
        tasks.insert(session_id.to_owned(), spawn());
        true
    }

    /// Makes a newly submitted turn's stream the canonical consumer. Any
    /// idle/reconnect consumer opened before the provider accepted the turn is
    /// aborted so two tasks never compete for the same provider event feed.
    pub fn replace(&self, session_id: &str, spawn: impl FnOnce() -> tokio::task::JoinHandle<()>) {
        let mut tasks = self.tasks.lock().expect("provider consumers lock");
        if let Some(previous) = tasks.remove(session_id) {
            previous.abort();
        }
        tasks.insert(session_id.to_owned(), spawn());
    }

    pub fn remove(&self, session_id: &str) {
        let _ = self
            .tasks
            .lock()
            .expect("provider consumers lock")
            .remove(session_id);
    }
}

/// Per-session broadcast channels for gateway-local events (approvals, …)
/// that don't originate from the runtime provider stream. SSE handlers merge
/// a receiver into the provider stream so local events reach the browser
/// without polling.
#[derive(Debug, Default)]
pub struct LocalSessionEvents {
    channels: Mutex<HashMap<String, broadcast::Sender<serde_json::Value>>>,
}

impl LocalSessionEvents {
    const CAPACITY: usize = 64;

    pub fn subscribe(&self, session_id: &str) -> broadcast::Receiver<serde_json::Value> {
        let mut channels = self.channels.lock().expect("local session events lock");
        channels
            .entry(session_id.to_owned())
            .or_insert_with(|| broadcast::channel(Self::CAPACITY).0)
            .subscribe()
    }

    /// Best-effort publish; drops the event when nobody is subscribed and
    /// garbage-collects idle channels so the map doesn't grow unbounded.
    pub fn publish(&self, session_id: &str, event: serde_json::Value) {
        let mut channels = self.channels.lock().expect("local session events lock");
        if let Some(sender) = channels.get(session_id) {
            if sender.send(event).is_err() {
                channels.remove(session_id);
            }
        }
    }
}

impl AppState {
    pub fn build_http_client() -> Result<Client, GatewayError> {
        Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .tcp_nodelay(true)
            .http2_adaptive_window(true)
            .build()
            .map_err(GatewayError::HttpClient)
    }

    pub fn new(
        config: GatewayConfig,
        router: Router,
        http: Client,
        model_cost_map: ModelCostMap,
        db: Option<PgPool>,
    ) -> Result<Self, GatewayError> {
        let callbacks = callbacks(&config, db.clone());
        let object_storage = ObjectStorageClient::from_settings(&config.general_settings);
        Ok(Self {
            mcp_servers: McpServerRegistry::from_config(&config)?,
            config,
            router,
            http,
            model_cost_map,
            agent_runs: AgentRunStore::default(),
            keyed_locks: KeyedLockStore::default(),
            db,
            api_keys: GatewayApiKeyStore::default(),
            callbacks,
            object_storage,
            local_session_events: LocalSessionEvents::default(),
            provider_consumers: ProviderConsumers::default(),
            mcp_proxy_base_url: RwLock::new(None),
        })
    }

    pub fn set_mcp_proxy_base_url_override(&self, value: Option<String>) {
        if let Ok(mut guard) = self.mcp_proxy_base_url.write() {
            *guard = value;
        }
    }

    pub fn mcp_proxy_base_url_override(&self) -> Option<String> {
        self.mcp_proxy_base_url
            .read()
            .ok()
            .and_then(|value| value.clone())
    }

    pub fn configured_mcp_proxy_base_url(&self) -> Option<String> {
        self.config
            .mcp_servers
            .proxy_base_url()
            .or(self.config.general_settings.public_base_url.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    }

    pub fn resolved_mcp_proxy_base_url(&self) -> Option<String> {
        self.mcp_proxy_base_url_override()
            .or_else(|| self.configured_mcp_proxy_base_url())
    }
}

fn callbacks(config: &GatewayConfig, db: Option<PgPool>) -> CallbackManager {
    let Some(pool) = db else {
        return CallbackManager::default();
    };
    if config.general_settings.disable_spend_logs {
        return CallbackManager::default();
    }
    CallbackManager::new(vec![Arc::new(LiteLLMDBCallback::new(
        pool,
        &config.general_settings,
    ))])
}
