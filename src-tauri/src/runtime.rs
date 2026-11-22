use crate::endpoint_policy;
use crate::known_hosts::KnownHosts;
use crate::model::{
    AppBlockReason, AppStatus, DockSnapshot, EndpointFailure, EndpointSelection, ForwardApp,
    TestState, TunnelConfig, TunnelEndpoint, TunnelPhase, TunnelTestResult,
};
use crate::network;
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

/// 一轮连接的结果：成功持有连接，或所有候选线路都失败。
struct RoundOutcome {
    connection: Option<ssh::SshTunnel>,
    failures: Vec<EndpointFailure>,
    /// 无法换线路解决的问题（安全 / 本地端口 / 凭据），一经出现就结束本轮。
    fatal: Option<TunnelError>,
    security_alert: bool,
    /// 最后一条尝试的线路，用于在错误详情页指出“是哪条路失败”。
    last_endpoint: Option<TunnelEndpoint>,
}

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
    /// 每个应用上次成功承载流量的线路 id：重连时优先试它，避免反复探测刚失败的路径。
    last_good: RwLock<HashMap<String, String>>,
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
            last_good: RwLock::new(HashMap::new()),
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

    /// Opens a throwaway SSH session per endpoint (no local forward) and closes
    /// it, so "测试连接" exercises auth, the jump chain and host key checks
    /// without touching any running application. Lines are tried in policy
    /// order and the first success wins; every failure is reported when none
    /// works.
    pub async fn test_tunnel(&self, id: &str) -> TunnelTestResult {
        let started = Instant::now();
        let snapshot = self.store.snapshot().await;
        let Some(tunnel) = snapshot.tunnel(id) else {
            return TunnelTestResult {
                tunnel_id: id.to_string(),
                state: TestState::Failure,
                elapsed_ms: 0,
                error_code: Some("tunnel_deleted".into()),
                error_detail: None,
                security_alert: false,
                endpoint_label: None,
                endpoint_failures: Vec::new(),
            };
        };
        // 跳板机的凭据可能来自定义同一主机的另一条隧道，测试与真实连接共用同一套解析。
        let tunnel = snapshot.with_inherited_jump_credentials(tunnel);
        let on_local_network = network::on_local_network();
        let candidates = endpoint_policy::order(
            &tunnel,
            tunnel.preferred_endpoint_id.as_deref(),
            None,
            on_local_network,
        );
        if candidates.is_empty() {
            let reason = endpoint_policy::no_candidate_reason(&tunnel, on_local_network);
            tracing::warn!(tunnel = %tunnel.name, code = reason.error_code(), "隧道没有可用线路");
            return TunnelTestResult {
                tunnel_id: id.to_string(),
                state: TestState::Failure,
                elapsed_ms: started.elapsed().as_millis() as u64,
                error_code: Some(reason.error_code().to_string()),
                error_detail: None,
                security_alert: false,
                endpoint_label: None,
                endpoint_failures: Vec::new(),
            };
        }

        let context = self.connect_context();
        let mut failures: Vec<EndpointFailure> = Vec::new();
        let mut security_alert = false;
        // 用普通循环而不是迭代器：安全类失败必须真的中断探测，
        // 否则下一条线路会把真正的原因盖住。
        for endpoint in candidates {
            match ssh::probe(&context, &tunnel, endpoint).await {
                Ok(()) => {
                    tracing::info!(
                        tunnel = %tunnel.name,
                        endpoint = %endpoint.display_name(),
                        "隧道连接测试成功"
                    );
                    return TunnelTestResult {
                        tunnel_id: id.to_string(),
                        state: TestState::Success,
                        elapsed_ms: started.elapsed().as_millis() as u64,
                        error_code: None,
                        error_detail: None,
                        security_alert,
                        endpoint_label: Some(endpoint.display_name().to_string()),
                        endpoint_failures: failures,
                    };
                }
                Err(error) => {
                    tracing::warn!(
                        tunnel = %tunnel.name,
                        endpoint = %endpoint.display_name(),
                        code = error.code(),
                        detail = error.detail().unwrap_or_default(),
                        "隧道连接测试失败"
                    );
                    security_alert |= error.security_alert();
                    failures.push(endpoint_failure(endpoint, &error));
                    if !endpoint_policy::is_endpoint_scoped(&error) {
                        break;
                    }
                }
            }
        }

        let first = failures
            .first()
            .expect("an empty candidate list returned above");
        TunnelTestResult {
            tunnel_id: id.to_string(),
            state: TestState::Failure,
            elapsed_ms: started.elapsed().as_millis() as u64,
            error_code: Some(first.code.clone()),
            error_detail: first.detail.clone(),
            security_alert,
            endpoint_label: None,
            endpoint_failures: failures,
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

    /// Supervises one application: resolve the endpoint list for this round,
    /// dial the first line that works, keep it healthy, and fall to the next
    /// line when a path dies. The local port never changes, so whoever is
    /// calling `127.0.0.1:port` does not notice the path switch. Only when
    /// *every* candidate failed does the round back off.
    async fn run_app(
        self: Arc<Self>,
        app: ForwardApp,
        token: CancellationToken,
        generation: u64,
        done: Arc<Notify>,
    ) {
        let context = self.connect_context();
        let mut attempt: u32 = 0;
        let mut last_round: Option<RoundOutcome> = None;
        let app_id = app.id.clone();

        loop {
            if token.is_cancelled() {
                break;
            }
            if attempt > 0 {
                let wait = ReconnectPolicy::backoff_seconds(attempt);
                let failure = last_round
                    .as_ref()
                    .and_then(|round| round.failures.last())
                    .cloned();
                self.set_status(
                    AppStatus::reconnecting(
                        &app_id,
                        attempt,
                        wait,
                        app.local_port,
                        failure.as_ref().map(|item| item.code.clone()),
                        failure.as_ref().and_then(|item| item.detail.clone()),
                        last_round.as_ref().map(block_reason_for).unwrap_or(AppBlockReason::None),
                    )
                    .with_endpoint_route(
                        last_round.as_ref().and_then(selected_endpoint),
                        last_round
                            .as_ref()
                            .map(|round| round.failures.clone())
                            .unwrap_or_default(),
                    ),
                )
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
                self.forget_last_good(&app_id).await;
                self.set_status(AppStatus::blocked(
                    &app_id,
                    current.local_port,
                    AppBlockReason::TunnelDeleted,
                ))
                .await;
                break;
            };
            // 跳板机的凭据可能由另一条隧道提供，连接前统一解析出来。
            let tunnel = snapshot.with_inherited_jump_credentials(&tunnel);

            let forward = ForwardSpec::from(&current);
            let last_good = self.last_good_endpoint(&app_id).await;
            let on_local_network = network::on_local_network();
            let candidates = endpoint_policy::order(
                &tunnel,
                tunnel.preferred_endpoint_id.as_deref(),
                last_good.as_deref(),
                on_local_network,
            );
            if candidates.is_empty() {
                // 配置层面就拨不出去（没有线路 / 全停用 / 缺凭据）：别说成“正在重连”，
                // 直接停下并说明原因，否则用户只会看到一直转圈。
                let reason = endpoint_policy::no_candidate_reason(&tunnel, on_local_network);
                tracing::warn!(
                    app = %current.name,
                    code = reason.error_code(),
                    "应用没有可用线路，停止重连"
                );
                self.store.set_app_desired(&app_id, false).await.ok();
                self.forget_last_good(&app_id).await;
                self.set_status(AppStatus::error(
                    &app_id,
                    current.local_port,
                    reason.error_code().to_string(),
                    None,
                    false,
                    AppBlockReason::TunnelDown,
                ))
                .await;
                break;
            }

            // The per-candidate "connecting" status is published inside the dial
            // loop below, right before each attempt.

            let mut round = RoundOutcome {
                connection: None,
                failures: Vec::new(),
                fatal: None,
                security_alert: false,
                last_endpoint: None,
            };
            for endpoint in candidates {
                if token.is_cancelled() {
                    break;
                }
                // 报告“正在连接哪一条线路”：第一条只管说“连接中”，后续的说明是在
                // 回退，界面据此显示“首选线路不可用，正在尝试 …”。
                self.set_status(
                    AppStatus::connecting(&app_id, current.local_port).with_endpoint_route(
                        Some(EndpointSelection::of(endpoint)),
                        round.failures.clone(),
                    ),
                )
                .await;
                let result = tokio::select! {
                    _ = token.cancelled() => break,
                    result = ssh::connect(&context, &tunnel, endpoint, &forward) => result,
                };
                match result {
                    Ok(connection) => {
                        round.connection = Some(connection);
                        round.last_endpoint = Some(endpoint.clone());
                        break;
                    }
                    Err(error) => {
                        tracing::warn!(
                            app = %current.name,
                            endpoint = %endpoint.display_name(),
                            code = error.code(),
                            detail = error.detail().unwrap_or_default(),
                            "应用线路连接失败"
                        );
                        round.security_alert |= error.security_alert();
                        round.failures.push(endpoint_failure(endpoint, &error));
                        round.last_endpoint = Some(endpoint.clone());
                        // 安全类 / 本地端口类问题换线路解决不了，还会把真正的原因盖住。
                        if !endpoint_policy::is_endpoint_scoped(&error) {
                            round.fatal = Some(error);
                            break;
                        }
                    }
                }
            }
            if token.is_cancelled() {
                if let Some(mut connection) = round.connection.take() {
                    connection.close().await;
                }
                break;
            }

            match round.connection.take() {
                Some(mut connection) => {
                    attempt = 0;
                    let endpoint = round
                        .last_endpoint
                        .as_ref()
                        .expect("a connection always records its endpoint")
                        .clone();
                    self.remember_last_good(&app_id, &endpoint.id).await;
                    self.set_status(
                        AppStatus::connected(&app_id, connection.local_port())
                            .with_endpoint_route(
                                Some(EndpointSelection::of(&endpoint)),
                                round.failures.clone(),
                            ),
                    )
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
                        endpoint = %endpoint.display_name(),
                        reason = %reason,
                        "应用转发中断"
                    );
                    round.security_alert = reason.security_alert();
                    round.failures = vec![endpoint_failure(&endpoint, &reason)];
                    round.fatal = Some(reason);
                    round.last_endpoint = Some(endpoint);
                    last_round = Some(round);
                }
                None => last_round = Some(round),
            }

            let round = last_round
                .as_ref()
                .expect("every round ends with either a connection or failures");
            // 只有“换条线路也没用”的失败才终止整个应用：安全类、本地端口类、
            // 凭据不可用，以及用户关掉了自动重连。
            let fatal = round.fatal.as_ref();
            let must_stop = round.security_alert
                || !tunnel.auto_reconnect
                || fatal.is_some_and(|error| {
                    endpoint_policy::is_security_failure(error)
                        || endpoint_policy::is_local_port_failure(error)
                        || matches!(
                            error,
                            TunnelError::MissingCredential | TunnelError::SecretDecrypt
                        )
                });
            if must_stop {
                let code = fatal
                    .map(|error| error.code().to_string())
                    .or_else(|| round.failures.last().map(|item| item.code.clone()))
                    .unwrap_or_else(|| "connection_closed".into());
                self.store.set_app_desired(&app_id, false).await.ok();
                self.forget_last_good(&app_id).await;
                self.set_status(
                    AppStatus::error(
                        &app_id,
                        current.local_port,
                        code,
                        round.failures.last().and_then(|item| item.detail.clone()),
                        round.security_alert,
                        block_reason_for(round),
                    )
                    .with_endpoint_route(selected_endpoint(round), round.failures.clone()),
                )
                .await;
                break;
            }
            attempt += 1;
        }

        self.forget_last_good(&app_id).await;
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

    async fn last_good_endpoint(&self, app_id: &str) -> Option<String> {
        self.last_good.read().await.get(app_id).cloned()
    }

    async fn remember_last_good(&self, app_id: &str, endpoint_id: &str) {
        self.last_good
            .write()
            .await
            .insert(app_id.to_string(), endpoint_id.to_string());
    }

    async fn forget_last_good(&self, app_id: &str) {
        self.last_good.write().await.remove(app_id);
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

/// 一轮连接的结果应当被判定为哪种应用阻塞原因。
fn block_reason_for(round: &RoundOutcome) -> AppBlockReason {
    match round.fatal.as_ref() {
        Some(TunnelError::PortInUse) => AppBlockReason::PortConflict,
        Some(TunnelError::LocalForwardLost) | Some(TunnelError::Io(_)) => {
            AppBlockReason::ForwardFailed
        }
        Some(_) => AppBlockReason::TunnelDown,
        None => AppBlockReason::TunnelDown,
    }
}

/// 本轮最后一次被尝试的线路，错误详情页据此指出“是哪条路失败”。
fn selected_endpoint(round: &RoundOutcome) -> Option<EndpointSelection> {
    round.last_endpoint.as_ref().map(EndpointSelection::of)
}

/// 把一次失败压成前端可见的、语言无关的记录。
fn endpoint_failure(endpoint: &TunnelEndpoint, error: &TunnelError) -> EndpointFailure {
    EndpointFailure {
        endpoint_id: endpoint.id.clone(),
        label: endpoint.display_name().to_string(),
        code: error.code().to_string(),
        detail: error.detail(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        block_reason_for, blocking_reason, endpoint_failure, selected_endpoint, ReconnectPolicy,
        RoundOutcome,
    };
    use crate::model::{AppBlockReason, AppGroup, DockSnapshot, ForwardApp, TunnelConfig};
    use crate::ssh::TunnelError;

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
            endpoints: vec![crate::model::TunnelEndpoint {
                host: "example.com".into(),
                username: "ops".into(),
                credentials: crate::model::SshCredentials {
                    auth_type: crate::model::AuthType::Password,
                    password_cipher: Some("v1.aa.bb".into()),
                    ..crate::model::SshCredentials::default()
                },
                ..crate::model::TunnelEndpoint::default()
            }],
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

    fn round_with(fatal: Option<TunnelError>) -> RoundOutcome {
        RoundOutcome {
            connection: None,
            failures: Vec::new(),
            fatal,
            security_alert: false,
            last_endpoint: None,
        }
    }

    #[test]
    fn a_local_port_failure_blocks_the_application_as_a_port_conflict() {
        assert_eq!(
            block_reason_for(&round_with(Some(TunnelError::PortInUse))),
            AppBlockReason::PortConflict
        );
    }

    #[test]
    fn a_lost_forward_is_reported_as_a_forward_failure() {
        assert_eq!(
            block_reason_for(&round_with(Some(TunnelError::LocalForwardLost))),
            AppBlockReason::ForwardFailed
        );
    }

    #[test]
    fn a_security_failure_is_reported_as_a_tunnel_failure() {
        assert_eq!(
            block_reason_for(&round_with(Some(TunnelError::HostKeyChanged))),
            AppBlockReason::TunnelDown
        );
    }

    /// 一轮连接把每条线路的失败压成前端可读的记录，线路名与错误码都要在。
    #[test]
    fn a_failed_endpoint_is_reported_with_its_label_and_code() {
        let endpoint = crate::model::TunnelEndpoint {
            label: "家庭局域网".into(),
            host: "192.168.1.10".into(),
            username: "ops".into(),
            ..crate::model::TunnelEndpoint::default()
        };
        let failure = endpoint_failure(&endpoint, &TunnelError::Timeout);
        assert_eq!(failure.label, "家庭局域网");
        assert_eq!(failure.code, "timeout");
        assert_eq!(failure.endpoint_id, endpoint.id);
        assert!(failure.detail.is_none());

        // 没有备注名时用主机地址，绝不出现空标签。
        let unnamed = crate::model::TunnelEndpoint {
            host: "frp.example.com".into(),
            ..crate::model::TunnelEndpoint::default()
        };
        assert_eq!(
            endpoint_failure(&unnamed, &TunnelError::Connect("boom".into())).label,
            "frp.example.com"
        );
    }

    #[test]
    fn the_failed_round_names_the_last_endpoint_it_tried() {
        let endpoint = crate::model::TunnelEndpoint {
            id: "away".into(),
            label: "frp 外网".into(),
            host: "frp.example.com".into(),
            username: "ops".into(),
            ..crate::model::TunnelEndpoint::default()
        };
        let mut round = round_with(None);
        round.last_endpoint = Some(endpoint.clone());
        let selected = selected_endpoint(&round).expect("the round tried a line");
        assert_eq!(selected.endpoint_id, "away");
        assert_eq!(selected.label, "frp 外网");
        assert_eq!(selected.endpoint, "ops@frp.example.com:22");

        assert!(selected_endpoint(&round_with(None)).is_none());
    }
}
