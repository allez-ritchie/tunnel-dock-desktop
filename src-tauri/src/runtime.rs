use crate::known_hosts::KnownHosts;
use crate::model::{TunnelConfig, TunnelPhase, TunnelStatus};
use crate::secrets::Secrets;
use crate::ssh::{self, ConnectContext, TunnelError};
use crate::store::Store;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, Notify, RwLock};
use tokio_util::sync::CancellationToken;

pub const STATUS_EVENT: &str = "tunnel-status";

/// Exponential reconnect delays: 1s, 2s, 4s, 8s, 16s, then a 30s ceiling.
pub struct ReconnectPolicy;

impl ReconnectPolicy {
    pub const MAX_SECONDS: u32 = 30;

    pub fn backoff_seconds(attempt: u32) -> u32 {
        let attempt = attempt.max(1);
        if attempt == 1 {
            return 1;
        }
        (1u32 << (attempt - 1).min(5)).min(Self::MAX_SECONDS)
    }
}

struct TaskEntry {
    token: CancellationToken,
    generation: u64,
    done: Arc<Notify>,
}

pub struct Runtime {
    app: AppHandle,
    store: Arc<Store>,
    secrets: Arc<Secrets>,
    known_hosts: Arc<KnownHosts>,
    statuses: RwLock<HashMap<String, TunnelStatus>>,
    tasks: Mutex<HashMap<String, TaskEntry>>,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    generation: AtomicU64,
}

impl Runtime {
    pub fn new(
        app: AppHandle,
        store: Arc<Store>,
        secrets: Arc<Secrets>,
        known_hosts: Arc<KnownHosts>,
    ) -> Arc<Self> {
        Arc::new(Self {
            app,
            store,
            secrets,
            known_hosts,
            statuses: RwLock::new(HashMap::new()),
            tasks: Mutex::new(HashMap::new()),
            locks: Mutex::new(HashMap::new()),
            generation: AtomicU64::new(0),
        })
    }

    pub async fn statuses_snapshot(&self) -> Vec<TunnelStatus> {
        let mut list: Vec<TunnelStatus> = self.statuses.read().await.values().cloned().collect();
        list.sort_by(|a, b| a.tunnel_id.cmp(&b.tunnel_id));
        list
    }

    pub async fn is_live(&self, id: &str) -> bool {
        self.statuses
            .read()
            .await
            .get(id)
            .map(|status| status.phase.is_live())
            .unwrap_or(false)
    }

    pub async fn connect(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let lock = self.operation_lock(id).await;
        let _guard = lock.lock().await;
        let config = self
            .store
            .get(id)
            .await
            .ok_or_else(|| "隧道配置不存在".to_string())?;
        self.store
            .set_desired(id, true)
            .await
            .map_err(|error| error.to_string())?;
        self.start_if_needed(config.with_desired(true)).await;
        Ok(())
    }

    pub async fn connect_all(self: &Arc<Self>) -> Result<(), String> {
        let configs = self.store.snapshot().await;
        let ids: Vec<String> = configs.iter().map(|config| config.id.clone()).collect();
        self.store
            .set_desired_for(&ids, true)
            .await
            .map_err(|error| error.to_string())?;
        for config in configs {
            let lock = self.operation_lock(&config.id).await;
            let _guard = lock.lock().await;
            self.start_if_needed(config.with_desired(true)).await;
        }
        Ok(())
    }

    pub async fn disconnect(self: &Arc<Self>, id: &str) {
        let lock = self.operation_lock(id).await;
        let _guard = lock.lock().await;
        self.store.set_desired(id, false).await.ok();
        let entry = self.tasks.lock().await.remove(id);
        if let Some(entry) = entry {
            entry.token.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(3), entry.done.notified()).await;
        }
        self.set_status(TunnelStatus::idle(id)).await;
    }

    pub async fn disconnect_all(self: &Arc<Self>) {
        self.store.set_all_desired(false).await.ok();
        let entries: Vec<TaskEntry> = {
            let mut tasks = self.tasks.lock().await;
            let ids: Vec<String> = tasks.keys().cloned().collect();
            ids.into_iter().filter_map(|id| tasks.remove(&id)).collect()
        };
        for entry in entries {
            entry.token.cancel();
        }
        let mut statuses = self.statuses.write().await;
        for status in statuses.values_mut() {
            *status = TunnelStatus::idle(&status.tunnel_id);
        }
        let snapshot: Vec<TunnelStatus> = statuses.values().cloned().collect();
        drop(statuses);
        for status in snapshot {
            self.emit(status);
        }
        self.refresh_tray().await;
    }

    pub async fn reconnect(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let lock = self.operation_lock(id).await;
        let _guard = lock.lock().await;
        let config = self
            .store
            .get(id)
            .await
            .ok_or_else(|| "隧道配置不存在".to_string())?;
        self.store
            .set_desired(id, true)
            .await
            .map_err(|error| error.to_string())?;
        let entry = self.tasks.lock().await.remove(id);
        if let Some(entry) = entry {
            entry.token.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(3), entry.done.notified()).await;
        }
        self.start_if_needed(config.with_desired(true)).await;
        Ok(())
    }

    pub async fn delete(self: &Arc<Self>, id: &str) {
        let lock = self.operation_lock(id).await;
        let _guard = lock.lock().await;
        self.store.set_desired(id, false).await.ok();
        let entry = self.tasks.lock().await.remove(id);
        if let Some(entry) = entry {
            entry.token.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(3), entry.done.notified()).await;
        }
        self.store.delete(id).await.ok();
        self.statuses.write().await.remove(id);
        self.refresh_tray().await;
    }

    /// App start: resume every tunnel that was on, plus tunnels opted into
    /// launch-at-login regardless of their last manual state.
    pub async fn restore_on_start(self: &Arc<Self>) {
        let configs = self.store.snapshot().await;
        for config in configs {
            if !config.desired_on && !config.auto_start_on_boot {
                continue;
            }
            let lock = self.operation_lock(&config.id).await;
            let _guard = lock.lock().await;
            if !config.desired_on {
                self.store.set_desired(&config.id, true).await.ok();
            }
            self.start_if_needed(config.with_desired(true)).await;
        }
    }

    pub async fn shutdown(self: &Arc<Self>) {
        let entries: Vec<TaskEntry> = {
            let mut tasks = self.tasks.lock().await;
            let ids: Vec<String> = tasks.keys().cloned().collect();
            ids.into_iter().filter_map(|id| tasks.remove(&id)).collect()
        };
        for entry in entries {
            entry.token.cancel();
        }
    }

    async fn start_if_needed(self: &Arc<Self>, config: TunnelConfig) {
        {
            let tasks = self.tasks.lock().await;
            if let Some(entry) = tasks.get(&config.id) {
                if !entry.token.is_cancelled() {
                    return;
                }
            }
        }
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let token = CancellationToken::new();
        let done = Arc::new(Notify::new());
        {
            let mut tasks = self.tasks.lock().await;
            tasks.insert(
                config.id.clone(),
                TaskEntry {
                    token: token.clone(),
                    generation,
                    done: done.clone(),
                },
            );
        }
        let runtime = self.clone();
        tokio::spawn(async move {
            runtime.run_tunnel(config, token, generation, done).await;
        });
    }

    async fn run_tunnel(
        self: Arc<Self>,
        config: TunnelConfig,
        token: CancellationToken,
        generation: u64,
        done: Arc<Notify>,
    ) {
        let context = ConnectContext {
            secrets: self.secrets.clone(),
            known_hosts: self.known_hosts.clone(),
        };
        let mut attempt: u32 = 0;
        let mut last_error: Option<TunnelError> = None;

        loop {
            if token.is_cancelled() {
                break;
            }
            if attempt > 0 {
                let wait = ReconnectPolicy::backoff_seconds(attempt);
                let (code, detail) = last_error
                    .as_ref()
                    .map(|error| (Some(error.code().to_string()), error.detail()))
                    .unwrap_or((None, None));
                self.set_status(TunnelStatus::reconnecting(
                    &config.id, attempt, wait, code, detail,
                ))
                .await;
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(Duration::from_secs(wait as u64)) => {}
                }
                if token.is_cancelled() {
                    break;
                }
            }

            let connect_result = tokio::select! {
                _ = token.cancelled() => break,
                result = ssh::connect(&context, &config) => result,
            };
            match connect_result {
                Ok(mut tunnel) => {
                    if token.is_cancelled() {
                        tunnel.close().await;
                        break;
                    }
                    attempt = 0;
                    self.set_status(TunnelStatus::connected(&config.id, tunnel.local_port()))
                        .await;
                    let reason = tokio::select! {
                        _ = token.cancelled() => {
                            tunnel.close().await;
                            break;
                        }
                        reason = tunnel.wait() => {
                            tunnel.close().await;
                            reason
                        }
                    };
                    if token.is_cancelled() {
                        break;
                    }
                    tracing::warn!(
                        tunnel = %config.name,
                        reason = %reason,
                        "隧道连接中断"
                    );
                    last_error = Some(reason);
                }
                Err(error) => {
                    tracing::warn!(
                        tunnel = %config.name,
                        code = error.code(),
                        detail = error.detail().unwrap_or_default(),
                        "隧道连接失败"
                    );
                    last_error = Some(error);
                }
            }

            let error = last_error
                .as_ref()
                .expect("loop always records an error before reconnecting");
            if error.security_alert() || !config.auto_reconnect {
                self.store.set_desired(&config.id, false).await.ok();
                self.set_status(TunnelStatus::error(
                    &config.id,
                    error.code().to_string(),
                    error.detail(),
                    error.security_alert(),
                ))
                .await;
                break;
            }
            attempt += 1;
        }

        if token.is_cancelled() {
            let desired = self
                .store
                .get(&config.id)
                .await
                .map(|config| config.desired_on)
                .unwrap_or(false);
            if !desired {
                self.set_status(TunnelStatus::idle(&config.id)).await;
            }
        }

        {
            let mut tasks = self.tasks.lock().await;
            if tasks
                .get(&config.id)
                .map(|entry| entry.generation)
                .is_some_and(|current| current == generation)
            {
                tasks.remove(&config.id);
            }
        }
        done.notify_one();
    }

    async fn set_status(&self, status: TunnelStatus) {
        self.statuses
            .write()
            .await
            .insert(status.tunnel_id.clone(), status.clone());
        self.emit(status);
        self.refresh_tray().await;
    }

    async fn refresh_tray(&self) {
        let statuses = self.statuses.read().await;
        let connected = statuses
            .values()
            .filter(|status| status.phase == TunnelPhase::Connected)
            .count();
        let reconnecting = statuses
            .values()
            .filter(|status| status.phase == TunnelPhase::Reconnecting)
            .count();
        drop(statuses);
        let tooltip = if reconnecting > 0 {
            format!("TunnelDock · {connected} 运行中 · {reconnecting} 重连中")
        } else if connected > 0 {
            format!("TunnelDock · {connected} 条隧道运行中")
        } else {
            "TunnelDock".to_string()
        };
        if let Some(tray) = self.app.tray_by_id("tray") {
            let _ = tray.set_tooltip(Some(tooltip));
        }
    }

    fn emit(&self, status: TunnelStatus) {
        if let Err(error) = self.app.emit(STATUS_EVENT, &status) {
            tracing::warn!(%error, "无法广播隧道状态");
        }
    }

    async fn operation_lock(&self, id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::ReconnectPolicy;

    #[test]
    fn early_attempts_retry_quickly() {
        assert_eq!(ReconnectPolicy::backoff_seconds(1), 1);
        assert_eq!(ReconnectPolicy::backoff_seconds(2), 2);
        assert_eq!(ReconnectPolicy::backoff_seconds(3), 4);
        assert_eq!(ReconnectPolicy::backoff_seconds(4), 8);
        assert_eq!(ReconnectPolicy::backoff_seconds(5), 16);
    }

    #[test]
    fn backoff_is_capped() {
        assert_eq!(ReconnectPolicy::backoff_seconds(6), 30);
        assert_eq!(ReconnectPolicy::backoff_seconds(50), 30);
    }

    #[test]
    fn non_positive_attempts_never_return_zero() {
        assert_eq!(ReconnectPolicy::backoff_seconds(0), 1);
    }
}
