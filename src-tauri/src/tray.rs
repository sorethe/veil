use crate::engine::Engine;
use crate::sysproxy;
use ksni::menu::{MenuItem, StandardItem};
use ksni::{Tray, TrayService};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

pub struct VeilTray {
    app: AppHandle,
    engine: Arc<Engine>,
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

impl Tray for VeilTray {
    fn id(&self) -> String {
        "veil".into()
    }

    fn title(&self) -> String {
        "Veil".into()
    }

    fn icon_name(&self) -> String {
        "network-vpn".into()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let status = self.engine.snapshot().status;
        ksni::ToolTip {
            title: "Veil".into(),
            description: format!("Engine: {status}"),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        show_window(&self.app);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let status = self.engine.snapshot().status;
        let running = matches!(status.as_str(), "connected" | "starting");
        let can_connect = matches!(status.as_str(), "idle" | "error");
        let can_disconnect = matches!(status.as_str(), "connected" | "starting" | "stopping");

        let show_app = self.app.clone();
        let connect_app = self.app.clone();
        let connect_engine = self.engine.clone();
        let disconnect_app = self.app.clone();
        let disconnect_engine = self.engine.clone();
        let quit_app = self.app.clone();
        let quit_engine = self.engine.clone();

        vec![
            MenuItem::Standard(StandardItem {
                label: format!("Engine: {status}"),
                enabled: false,
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(StandardItem {
                label: "Show Veil".into(),
                activate: Box::new(move |_| show_window(&show_app)),
                ..Default::default()
            }),
            MenuItem::Standard(StandardItem {
                label: if running {
                    "Connect (already connected)"
                } else {
                    "Connect"
                }
                .into(),
                enabled: can_connect,
                activate: Box::new(move |_| {
                    let app = connect_app.clone();
                    let engine = connect_engine.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = engine.connect(None, None).await {
                            let _ = app.emit(
                                "engine://notice",
                                serde_json::json!({ "level": "error", "message": e }),
                            );
                        }
                    });
                }),
                ..Default::default()
            }),
            MenuItem::Standard(StandardItem {
                label: "Disconnect".into(),
                enabled: can_disconnect,
                activate: Box::new(move |_| {
                    let app = disconnect_app.clone();
                    let engine = disconnect_engine.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(e) = engine.disconnect().await {
                            let _ = app.emit(
                                "engine://notice",
                                serde_json::json!({ "level": "error", "message": e }),
                            );
                        }
                    });
                }),
                ..Default::default()
            }),
            MenuItem::Separator,
            MenuItem::Standard(StandardItem {
                label: "Quit".into(),
                activate: Box::new(move |_| {
                    quit_engine.begin_shutdown_and_exit(quit_app.clone());
                }),
                ..Default::default()
            }),
        ]
    }
}

/// Spawn the StatusNotifierItem tray if a watcher host exists. Returns false
/// when the desktop has no tray host; the caller then uses normal window
/// behavior and never pretends a tray is available.
pub fn spawn(app: AppHandle, engine: Arc<Engine>) -> bool {
    if !sysproxy::tray_available().0 {
        return false;
    }
    let service = TrayService::new(VeilTray {
        app,
        engine: engine.clone(),
    });
    let handle = service.handle();
    service.spawn();
    // Keep the menu's connected/enabled state reasonably fresh.
    std::thread::spawn(move || {
        let mut last = String::new();
        loop {
            let status = engine.snapshot().status;
            if status != last {
                handle.update(|_| {});
                last = status;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    true
}
