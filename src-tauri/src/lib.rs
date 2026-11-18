mod commands;
mod known_hosts;
mod model;
mod runtime;
mod secrets;
mod ssh;
mod store;

use known_hosts::KnownHosts;
use runtime::Runtime;
use secrets::Secrets;
use std::sync::Arc;
use store::Store;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};

pub struct AppState {
    pub store: Arc<Store>,
    pub secrets: Arc<Secrets>,
    pub known_hosts: Arc<KnownHosts>,
    pub runtime: Arc<Runtime>,
}

pub(crate) async fn sync_autostart(app: &AppHandle) {
    use tauri_plugin_autostart::ManagerExt;

    let state = app.state::<AppState>();
    let wanted = state.store.any_auto_start().await;
    let manager = app.autolaunch();
    let result = if wanted {
        manager.enable()
    } else {
        manager.disable()
    };
    if let Err(error) = result {
        tracing::warn!(%error, "更新登录自启动失败");
    }
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let disconnect = MenuItem::with_id(app, "disconnect-all", "全部断开", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &disconnect,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?;

    TrayIconBuilder::with_id("tray")
        .icon(icon)
        .tooltip("TunnelDock")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_main_window(app),
            "disconnect-all" => {
                let runtime = app.state::<AppState>().runtime.clone();
                tauri::async_runtime::spawn(async move {
                    runtime.disconnect_all().await;
                });
            }
            "quit" => {
                let runtime = app.state::<AppState>().runtime.clone();
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    runtime.shutdown().await;
                    app.exit(0);
                });
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn run() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "tunnel_dock_lib=info,russh=warn".into());
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            let (store, secrets, known_hosts) = tauri::async_runtime::block_on(async {
                let store = Arc::new(Store::load(&data_dir).await?);
                let secrets = Secrets::load(&data_dir)?;
                let known_hosts = Arc::new(KnownHosts::load(&data_dir).await?);
                Ok::<_, Box<dyn std::error::Error>>((store, secrets, known_hosts))
            })?;

            let runtime = Runtime::new(
                handle.clone(),
                store.clone(),
                secrets.clone(),
                known_hosts.clone(),
            );
            app.manage(AppState {
                store,
                secrets,
                known_hosts,
                runtime: runtime.clone(),
            });

            setup_tray(app)?;

            let boot_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                runtime.restore_on_start().await;
                sync_autostart(&boot_handle).await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Closing the window keeps tunnels alive in the tray.
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tunnels,
            commands::list_statuses,
            commands::save_tunnel,
            commands::delete_tunnel,
            commands::connect_tunnel,
            commands::connect_all,
            commands::disconnect_tunnel,
            commands::disconnect_all,
            commands::reconnect_tunnel,
            commands::known_hosts_count,
            commands::clear_known_hosts,
            commands::open_url,
            commands::app_version,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TunnelDock");
}
