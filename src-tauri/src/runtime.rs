use crate::known_hosts::KnownHosts;
use crate::model::{
    AppBlockReason, AppStatus, DockSnapshot, ForwardApp, TestState, TunnelConfig, TunnelPhase,
    TunnelTestResult,
};
use crate::secrets::Secrets;
use crate::ssh::{self, ConnectContext, ForwardSpec, TunnelError};
use crate::store::Store;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, Notify, RwLock};
use tokio_util::sync::CancellationToken;

pub const APP_STATUS_EVENT: &str = "app-status";

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

/// Process-wide supervisor for forwarded applications.
///
/// A [`TunnelConfig`] is reusable *configuration*; a [`ForwardApp`] forwards
/// `local_port -> remote_host:remote_port` through one. Every running
/// application dials its own SSH session and owns its own local listener, so
/// applications that share a tunnel never share a transport.
pub struct Runtime {
    app: AppHandle,
    store: Arc<Store>,
    secrets: Arc<Secrets>,
    known_hosts: Arc<KnownHosts>,
    statuses: RwLock<HashMap<String, AppStatus>>,
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

    pub async fn statuses_snapshot(&self) -> Vec<AppStatus> {
        let mut list: Vec<AppStatus> = self.statuses.read().await.values().cloned().collect();
        list.sort_by(|a, b| a.app_id.cmp(&b.app_id));
        list
    }

    // ------------------------------------------------------------- lifecycle

    pub async fn start_app(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let lock = self.operation_lock(id).await;
        let _guard = lock.lock().await;
        let snapshot = self.store.snapshot().await;
        let app = snapshot
            .app(id)
            .cloned()
            .ok_or_else(|| "应用不存在".to_string())?;
        if let Some(reason) = blocking_reason(&snapshot, &app) {
            self.store.set_app_desired(id, false).await.ok();
            self.set_status(self.blocked_status(&app, reason)).await;
            return Err(blocked_message(reason).to_string());
        }
        self.store
            .set_app_desired(id, true)
            .await
            .map_err(|error| error.to_string())?;
        self.start_if_needed(app.clone().with_desired(true)).await;
        Ok(())
    }

    pub async fn stop_app(self: &Arc<Self>, id: &str) {
        let lock = self.operation_lock(id).await;
        let _guard = lock.lock().await;
        self.store.set_app_desired(id, false).await.ok();
        self.stop_task(id).await;
        let local_port = self
            .store
            .get_app(id)
            .await
            .map(|app| app.local_port)
            .unwrap_or(0);
        self.set_status(AppStatus {
            local_port: Some(local_port),
            ..AppStatus::idle(id)
        })
        .await;
    }

    pub async fn restart_app(self: &Arc<Self>, id: &str) -> Result<(), String> {
        let lock = self.operation_lock(id).await;
        let _guard = lock.lock().await;
        let snapshot = self.store.snapshot().await;
        let app = snapshot
            .app(id)
            .cloned()
            .ok_or_else(|| "应用不存在".to_string())?;
        if let Some(reason) = blocking_reason(&snapshot, &app) {
            self.store.set_app_desired(id, false).await.ok();
            self.set_status(self.blocked_status(&app, reason)).await;
            return Err(blocked_message(reason).to_string());
        }
        self.store
            .set_app_desired(id, true)
            .await
            .map_err(|error| error.to_string())?;
        self.stop_task(id).await;
        self.start_if_needed(app.clone().with_desired(true)).await;
        Ok(())
    }

    pub async fn start_group(self: &Arc<Self>, group_id: &str) -> Result<(), String> {
        let snapshot = self.store.snapshot().await;
        if snapshot.group(group_id).is_none() {
            return Err("应用组不存在".into());
        }
        let apps: Vec<String> = snapshot
            .apps
            .iter()
            .filter(|app| app.group_id == group_id)
            .map(|app| app.id.clone())
            .collect();
        // Start sequentially so the first application to claim a local port
        // wins and later duplicates become visibly blocked instead of all
        // failing together.
        for app_id in apps {
            if let Err(error) = self.start_app(&app_id).await {
                tracing::warn!(app = %app_id, %error, "应用组启动成员失败");
            }
        }
        Ok(())
    }

    pub async fn stop_group(self: &Arc<Self>, group_id: &str) {
        let snapshot = self.store.snapshot().await;
        let apps: Vec<String> = snapshot
            .apps
            .iter()
            .filter(|app| app.group_id == group_id)
            .map(|app| app.id.clone())
            .collect();
        for app_id in apps {
            self.stop_app(&app_id).await;
        }
    }

    pub async fn delete_app(self: &Arc<Self>, id: &str) {
        let lock = self.operation_lock(id).await;
        let _guard = lock.lock().await;
        self.store.set_app_desired(id, false).await.ok();
        self.stop_task(id).await;
        self.store.delete_app(id).await.ok();
        self.statuses.write().await.remove(id);
    }

    /// Applies an edited tunnel definition. Applications built from the old
    /// definition are stopped (never deleted) so the change cannot be hidden
    /// behind a live session; the user can restart them afterwards.
    pub async fn save_tunnel(
        self: &Arc<Self>,
        tunnel: TunnelConfig,
    ) -> Result<TunnelConfig, String> {
        let affected = self.store.snapshot().await.apps_using_tunnel(&tunnel.id);
        for app in affected {
            let lock = self.operation_lock(&app.id).await;
            let _guard = lock.lock().await;
            self.store.set_app_desired(&app.id, false).await.ok();
            self.stop_task(&app.id).await;
            self.set_status(AppStatus {
                local_port: Some(app.local_port),
                ..AppStatus::idle(&app.id)
            })
            .await;
        }
        self.store
            .upsert_tunnel(tunnel)
            .await
            .map_err(|error| error.to_string())
    }

    /// Stops every application that uses the tunnel, keeps the applications
    /// but marks them unavailable, then removes the tunnel definition.
    pub async fn delete_tunnel(self: &Arc<Self>, id: &str) {
        let snapshot = self.store.snapshot().await;
        for app in snapshot.apps_using_tunnel(id) {
            let lock = self.operation_lock(&app.id).await;
            let _guard = lock.lock().await;
            self.store.set_app_desired(&app.id, false).await.ok();
            self.stop_task(&app.id).await;
            self.set_status(AppStatus::blocked(
                &app.id,
                app.local_port,
                AppBlockReason::TunnelDeleted,
            ))
            .await;
        }
        self.store.delete_tunnel(id).await.ok();
    }

    /// Deletes a group and every application inside it after stopping them.
    pub async fn delete_group(self: &Arc<Self>, id: &str) -> Result<(), String> {
        if id == crate::model::DEFAULT_GROUP_ID {
            return Err("默认应用组不能删除".into());
        }
        let snapshot = self.store.snapshot().await;
        let apps: Vec<ForwardApp> = snapshot
            .apps
            .iter()
            .filter(|app| app.group_id == id)
            .cloned()
            .collect();
        let doomed: Vec<String> = apps.iter().map(|app| app.id.clone()).collect();
        for app in &apps {
            let lock = self.operation_lock(&app.id).await;
            let _guard = lock.lock().await;
            self.store.set_app_desired(&app.id, false).await.ok();
            self.stop_task(&app.id).await;
        }
        self.store
            .delete_group(id)
            .await
            .map_err(|error| error.to_string())?;
        let mut statuses = self.statuses.write().await;
        for app_id in doomed {
            statuses.remove(&app_id);
        }
        drop(statuses);
        self.refresh_tray().await;
        Ok(())
    }

    /// Opens a throwaway SSH session through the tunnel definition (no local
    /// forward) and closes it. Does not touch running applications.
    pub async fn test_tunnel(&self, id: &str) -> TunnelTestResult {
        let started = Instant::now();
        let Some(tunnel) = self.store.get_tunnel(id).await else {
            return TunnelTestResult {
                tunnel_id: id.to_string(),
                state: TestState::Failure,
                elapsed_ms: 0,
                error_code: Some("tunnel_deleted".into()),
                error_detail: None,
                security_alert: false,
            };
        };
        let context = self.connect_context();
        match ssh::probe(&context, &tunnel).await {
            Ok(()) => {
                tracing::info!(tunnel = %tunnel.name, "隧道连接测试成功");
                TunnelTestResult {
                    tunnel_id: id.to_string(),
                    state: TestState::Success,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    error_code: None,
                    error_detail: None,
                    security_alert: false,
                }
            }
            Err(error) => {
                tracing::warn!(
                    tunnel = %tunnel.name,
                    code = error.code(),
                    detail = error.detail().unwrap_or_default(),
                    "隧道连接测试失败"
                );
                TunnelTestResult {
                    tunnel_id: id.to_string(),
                    state: TestState::Failure,
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    error_code: Some(error.code().to_string()),
                    error_detail: error.detail(),
                    security_alert: error.security_alert(),
                }
            }
        }
    }

    // -------------------------------------------------------------- recovery

    /// Restores the persisted intent on application start. Dangling tunnel or
    /// group references are surfaced as blocked applications (never deleted),
    /// and boot-start applications opt in even if their last intent was off.
    pub async fn restore_on_start(self: &Arc<Self>) {
        let snapshot = self.store.snapshot().await;
        for app in &snapshot.apps {
            if let Some(reason) = blocking_reason(&snapshot, app) {
                self.store.set_app_desired(&app.id, false).await.ok();
                self.set_status(self.blocked_status(app, reason)).await;
            }
        }
        for app in &snapshot.apps {
            if !app.desired_on && !app.auto_start_on_boot {
                continue;
            }
            if blocking_reason(&snapshot, &app).is_some() {
                continue;
            }
            if !app.desired_on {
                self.store.set_app_desired(&app.id, true).await.ok();
            }
            let lock = self.operation_lock(&app.id).await;
            let _guard = lock.lock().await;
            self.start_if_needed(app.clone().with_desired(true)).await;
        }
    }

    /// Tray action: stop everything while keeping the persisted configuration.
    pub async fn disconnect_all(self: &Arc<Self>) {
        let snapshot = self.store.snapshot().await;
        let ids: Vec<String> = snapshot.apps.iter().map(|app| app.id.clone()).collect();
        self.store.set_apps_desired(&ids, false).await.ok();
        for app in snapshot.apps {
            let lock = self.operation_lock(&app.id).await;
            let _guard = lock.lock().await;
            self.stop_task(&app.id).await;
            self.set_status(AppStatus {
                local_port: Some(app.local_port),
                ..AppStatus::idle(&app.id)
            })
            .await;
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

    // -------------------------------------------------------------- internals

    fn connect_context(&self) -> ConnectContext {
        ConnectContext {
            secrets: self.secrets.clone(),
            known_hosts: self.known_hosts.clone(),
        }
    }

    async fn start_if_needed(self: &Arc<Self>, app: ForwardApp) {
        {
            let tasks = self.tasks.lock().await;
            if let Some(entry) = tasks.get(&app.id) {
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
                app.id.clone(),
                TaskEntry {
                    token: token.clone(),
                    generation,
                    done: done.clone(),
                },
            );
        }
        let runtime = self.clone();
        tokio::spawn(async move {
            runtime.run_app(app, token, generation, done).await;
        });
    }

    async fn run_app(
        self: Arc<Self>,
        app: ForwardApp,
        token: CancellationToken,
        generation: u64,
        done: Arc<Notify>,
    ) {
        let context = self.connect_context();
        let mut attempt: u32 = 0;
        let mut last_error: Option<TunnelError> = None;
        let app_id = app.id.clone();

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
                self.set_status(AppStatus::reconnecting(
                    &app_id,
                    attempt,
                    wait,
                    app.local_port,
                    code,
                    detail,
                    block_reason_for(last_error.as_ref()),
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

            let snapshot = self.store.snapshot().await;
            let Some(current) = snapshot.app(&app_id).cloned() else {
                break;
            };
            if !current.desired_on {
                break;
            }
            let Some(tunnel) = snapshot.tunnel(&current.tunnel_id).cloned() else {
                self.store.set_app_desired(&app_id, false).await.ok();
                self.set_status(AppStatus::blocked(
                    &app_id,
                    current.local_port,
                    AppBlockReason::TunnelDeleted,
                ))
                .await;
                break;
            };

            // Report "connecting" before dialling so the UI never sits on a
            // stale idle/error state while the first attempt is in flight.
            self.set_status(AppStatus::connecting(&app_id, current.local_port))
                .await;
            let forward = ForwardSpec::from(&current);
            let result = tokio::select! {
                _ = token.cancelled() => break,
                result = ssh::connect(&context, &tunnel, &forward) => result,
            };

            match result {
                Ok(mut connection) => {
                    if token.is_cancelled() {
                        connection.close().await;
                        break;
                    }
                    attempt = 0;
                    self.set_status(AppStatus::connected(&app_id, connection.local_port()))
                        .await;
                    let reason = tokio::select! {
                        _ = token.cancelled() => {
                            connection.close().await;
                            break;
                        }
                        reason = connection.wait() => {
                            connection.close().await;
                            reason
                        }
                    };
                    if token.is_cancelled() {
                        break;
                    }
                    tracing::warn!(
                        app = %current.name,
                        reason = %reason,
                        "应用转发中断"
                    );
                    last_error = Some(reason);
                }
                Err(error) => {
                    tracing::warn!(
                        app = %current.name,
                        code = error.code(),
                        detail = error.detail().unwrap_or_default(),
                        "应用连接失败"
                    );
                    last_error = Some(error);
                }
            }

            let error = last_error
                .as_ref()
                .expect("loop always records an error before reconnecting");
            if error.security_alert() || !tunnel.auto_reconnect {
                self.store.set_app_desired(&app_id, false).await.ok();
                self.set_status(AppStatus::error(
                    &app_id,
                    current.local_port,
                    error.code().to_string(),
                    error.detail(),
                    error.security_alert(),
                    block_reason_for(Some(error)),
                ))
                .await;
                break;
            }
            attempt += 1;
        }

        {
            let mut tasks = self.tasks.lock().await;
            if tasks
                .get(&app_id)
                .map(|entry| entry.generation)
                .is_some_and(|current| current == generation)
            {
                tasks.remove(&app_id);
            }
        }
        done.notify_one();
    }

    /// Cancels the running supervisor and waits for it, so its teardown can
    /// never overwrite a status the caller publishes right after.
    async fn stop_task(&self, id: &str) {
        let entry = self.tasks.lock().await.remove(id);
        if let Some(entry) = entry {
            entry.token.cancel();
            let _ = tokio::time::timeout(Duration::from_secs(3), entry.done.notified()).await;
        }
    }

    fn blocked_status(&self, app: &ForwardApp, reason: AppBlockReason) -> AppStatus {
        AppStatus::blocked(&app.id, app.local_port, reason)
    }

    async fn set_status(&self, status: AppStatus) {
        self.statuses
            .write()
            .await
            .insert(status.app_id.clone(), status.clone());
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
        let connecting = statuses
            .values()
            .filter(|status| status.phase == TunnelPhase::Connecting)
            .count();
        drop(statuses);
        let mut parts: Vec<String> = Vec::new();
        if connected > 0 {
            parts.push(format!("{connected} 转发中"));
        }
        if reconnecting > 0 {
            parts.push(format!("{reconnecting} 重连中"));
        }
        if connecting > 0 {
            parts.push(format!("{connecting} 准备中"));
        }
        let tooltip = if parts.is_empty() {
            "TunnelDock".to_string()
        } else {
            format!("TunnelDock · {}", parts.join(" · "))
        };
        if let Some(tray) = self.app.tray_by_id("tray") {
            let _ = tray.set_tooltip(Some(tooltip));
        }
    }

    fn emit(&self, status: AppStatus) {
        if let Err(error) = self.app.emit(APP_STATUS_EVENT, &status) {
            tracing::warn!(%error, "无法广播应用状态");
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

/// Why an application cannot start right now, given the current definitions.
fn blocking_reason(snapshot: &DockSnapshot, app: &ForwardApp) -> Option<AppBlockReason> {
    if snapshot.tunnel(&app.tunnel_id).is_none() {
        return Some(AppBlockReason::TunnelDeleted);
    }
    if snapshot.group(&app.group_id).is_none() {
        return Some(AppBlockReason::GroupMissing);
    }
    if snapshot
        .apps
        .iter()
        .any(|other| other.id != app.id && other.local_port == app.local_port && other.desired_on)
    {
        return Some(AppBlockReason::PortConflict);
    }
    None
}

fn blocked_message(reason: AppBlockReason) -> &'static str {
    match reason {
        AppBlockReason::TunnelDeleted => "应用使用的隧道已被删除，请重新选择隧道",
        AppBlockReason::GroupMissing => "应用所属的应用组不存在",
        AppBlockReason::PortConflict => "本地端口已被其他应用占用",
        AppBlockReason::TunnelDown => "隧道连接失败",
        AppBlockReason::ForwardFailed => "转发建立失败",
        AppBlockReason::None => "应用不可用",
    }
}

fn block_reason_for(error: Option<&TunnelError>) -> AppBlockReason {
    match error {
        Some(TunnelError::PortInUse) => AppBlockReason::PortConflict,
        Some(TunnelError::LocalForwardLost) | Some(TunnelError::Io(_)) => {
            AppBlockReason::ForwardFailed
        }
        Some(_) => AppBlockReason::TunnelDown,
        None => AppBlockReason::None,
    }
}

#[cfg(test)]
mod tests {
    use super::{blocking_reason, ReconnectPolicy};
    use crate::model::{AppBlockReason, AppGroup, DockSnapshot, ForwardApp, TunnelConfig};

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

    fn document() -> DockSnapshot {
        let tunnel = TunnelConfig {
            id: "t1".into(),
            name: "NAS".into(),
            ssh_host: "example.com".into(),
            ssh_user: "ops".into(),
            ..TunnelConfig::default()
        };
        let app = ForwardApp {
            id: "a1".into(),
            name: "Web".into(),
            tunnel_id: "t1".into(),
            group_id: "default".into(),
            local_port: 18080,
            remote_host: "127.0.0.1".into(),
            ..ForwardApp::default()
        };
        DockSnapshot {
            tunnels: vec![tunnel],
            apps: vec![app],
            groups: vec![AppGroup::default_group()],
            ..DockSnapshot::default()
        }
        .normalized()
    }

    #[test]
    fn start_validation_accepts_a_complete_app() {
        let snapshot = document();
        assert_eq!(blocking_reason(&snapshot, &snapshot.apps[0]), None);
    }

    #[test]
    fn start_validation_detects_deleted_tunnel() {
        let mut snapshot = document();
        snapshot.tunnels.clear();
        assert_eq!(
            blocking_reason(&snapshot, &snapshot.apps[0]),
            Some(AppBlockReason::TunnelDeleted)
        );
    }

    #[test]
    fn start_validation_detects_port_conflict_between_desired_apps() {
        let mut snapshot = document();
        let mut other = snapshot.apps[0].clone();
        other.id = "a2".into();
        other.name = "API".into();
        other.desired_on = true;
        snapshot.apps.push(other);
        assert_eq!(
            blocking_reason(&snapshot, &snapshot.apps[0]),
            Some(AppBlockReason::PortConflict)
        );
    }

    #[test]
    fn start_validation_detects_missing_group() {
        let mut snapshot = document();
        snapshot.apps[0].group_id = "gone".into();
        // A missing group is normalized back to default when the document is
        // written, so build the snapshot directly to exercise the guard.
        assert_eq!(
            blocking_reason(&snapshot, &snapshot.apps[0]),
            Some(AppBlockReason::GroupMissing)
        );
    }
}
