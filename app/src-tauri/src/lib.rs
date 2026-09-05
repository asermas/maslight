//! The MasLight desktop application.
//!
//! Everything interesting happens in `maslight-engine`; this crate is the
//! shell around it: the window, the tray, the commands the interface calls,
//! the optional local API, and the small amount of platform glue for starting
//! at login.

use std::sync::Arc;

use maslight_api::ApiServer;
use maslight_core::{AppConfig, Layout, Rgb8, WizardParams};
use maslight_engine::{EngineHandle, EngineStatus};
use parking_lot::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

mod api_bridge;
mod autostart;
mod commands;

/// State the interface, the tray and the API all share.
///
/// Held behind an `Arc` because the API server outlives any single request and
/// needs its own handle on the same configuration.
pub struct Shared {
    pub config: Mutex<AppConfig>,
    pub engine: Mutex<Arc<EngineHandle>>,
}

impl Shared {
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
}

/// What Tauri manages.
pub struct AppState {
    pub shared: Arc<Shared>,
    /// The local API server, when the configuration asks for one.
    pub api: Mutex<Option<ApiServer>>,
}

impl AppState {
    fn new(config: AppConfig) -> Self {
        let engine = EngineHandle::spawn(config.clone());
        Self {
            shared: Arc::new(Shared {
                config: Mutex::new(config),
                engine: Mutex::new(Arc::new(engine)),
            }),
            api: Mutex::new(None),
        }
    }

    pub fn config(&self) -> AppConfig {
        self.shared.config.lock().clone()
    }

    /// Persist, hand to the engine, and bring the API server in line.
    pub fn commit(&self, config: AppConfig) -> Result<(), String> {
        self.shared.commit(config)?;
        let mut server = self.api.lock();
        api_bridge::reconcile(&self.shared, &mut server);
        Ok(())
    }

    pub fn status(&self) -> EngineStatus {
        self.shared.engine.lock().status()
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

    let mut config = AppConfig::load_or_default();
    // A configuration that asks for the API but carries no token would refuse
    // to start the server, so fill one in before anything reads it.
    if config.ui.api_token.trim().is_empty() {
        config.ui.api_token = maslight_api::generate_token();
        // Write it straight away: a token that only exists in memory would
        // change on every launch and break every script that stored it.
        if let Err(e) = config.save_to(&maslight_core::profile::config_path()) {
            tracing::warn!("could not store the API token: {e}");
        }
    }
    let start_minimised = config.ui.start_minimised;
    let state = AppState::new(config);
    {
        let mut server = state.api.lock();
        api_bridge::reconcile(&state.shared, &mut server);
    }

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
            commands::list_script_examples,
            commands::discover_devices,
            commands::probe_device,
            commands::build_layout,
            commands::identify_led,
            commands::hold_color,
            commands::set_launch_at_login,
            commands::regenerate_api_token,
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
                let mut config = state.config();
                config.enabled = !config.enabled;
                let enabled = config.enabled;
                if let Err(e) = state.commit(config) {
                    tracing::warn!("could not toggle from the tray: {e}");
                }
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
    maslight_api::parse_hex(value)
}
