//! The MasLight desktop application.
//!
//! Everything interesting happens in `maslight-engine`; this crate is the
//! shell around it: the window, the tray, the commands the interface calls,
//! and the small amount of platform glue for starting at login.

use std::sync::Arc;

use maslight_core::{AppConfig, Layout, Rgb8, WizardParams};
use maslight_engine::{EngineHandle, EngineStatus};
use parking_lot::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

mod autostart;
mod commands;

/// Shared application state.
pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub engine: Mutex<Arc<EngineHandle>>,
}

impl AppState {
    fn new(config: AppConfig) -> Self {
        let engine = EngineHandle::spawn(config.clone());
        Self {
            config: Mutex::new(config),
            engine: Mutex::new(Arc::new(engine)),
        }
    }

    /// Persist the configuration and hand it to the engine.
    pub fn commit(&self, config: AppConfig) -> Result<(), String> {
        let mut config = config;
        config.sanitise();
        config
            .save_to(&maslight_core::profile::config_path())
            .map_err(|e| e.to_string())?;
        self.engine.lock().apply(config.clone());
        *self.config.lock() = config;
        Ok(())
    }

    pub fn status(&self) -> EngineStatus {
        self.engine.lock().status()
    }
}

/// Entry point shared by the desktop binary and any future embedder.
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("MASLIGHT_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = AppConfig::load_or_default();
    let start_minimised = config.ui.start_minimised;
    let state = AppState::new(config);

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_status,
            commands::set_enabled,
            commands::set_active_profile,
            commands::delete_profile,
            commands::list_displays,
            commands::list_capture_backends,
            commands::list_audio_devices,
            commands::discover_devices,
            commands::probe_device,
            commands::build_layout,
            commands::identify_led,
            commands::hold_color,
            commands::set_launch_at_login,
            commands::app_info,
            commands::open_config_dir,
        ])
        .setup(move |app| {
            build_tray(app.handle())?;
            if let Some(window) = app.get_webview_window("main") {
                if start_minimised {
                    let _ = window.hide();
                }
                // Closing the window leaves the engine running, which is what
                // an always-on light is expected to do.
                let handle = app.handle().clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        if let Some(w) = handle.get_webview_window("main") {
                            let _ = w.hide();
                        }
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to start MasLight");
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Open MasLight", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Lights on / off", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &separator, &quit])?;

    TrayIconBuilder::with_id("maslight")
        .icon(app.default_window_icon().cloned().unwrap())
        .tooltip("MasLight")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main(app),
            "toggle" => {
                let state: State<'_, AppState> = app.state();
                let enabled = !state.config.lock().enabled;
                {
                    let mut cfg = state.config.lock();
                    cfg.enabled = enabled;
                }
                state.engine.lock().set_enabled(enabled);
                let _ = app.emit("maslight://enabled", enabled);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Build a layout from wizard parameters. Exposed here so the command module
/// and any future headless CLI share one implementation.
pub fn layout_from_wizard(params: &WizardParams) -> Layout {
    Layout::from_wizard(params)
}

/// Parse a `#rrggbb` string into a colour.
pub fn parse_hex(value: &str) -> Option<Rgb8> {
    let v = value.trim().trim_start_matches('#');
    if v.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(v, 16).ok()?;
    Some(Rgb8::new(
        ((n >> 16) & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        (n & 0xff) as u8,
    ))
}
