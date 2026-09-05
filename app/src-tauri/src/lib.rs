//! The MasLight desktop application.
//!
//! Everything interesting happens in `maslight-engine`; this crate is the
//! shell around it: the window, the tray, the commands the interface calls,
//! the optional local API, and the small amount of platform glue for starting
//! at login.

use std::sync::Arc;

use maslight_api::ApiServer;
use maslight_core::profile::ConfigLoad;
use maslight_core::{AppConfig, Layout, Rgb8, WizardParams};
use maslight_engine::{EngineHandle, EngineStatus};
use parking_lot::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

mod api_bridge;
mod autostart;
mod commands;
mod discovery;

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

/// Send the log to a file as well as to the console.
///
/// A desktop application started from a shortcut, from the tray, or at login
/// has no console to write to, so on Windows the log went nowhere at all and
/// there was no way to find out why anything had happened. Keeping a file
/// next to the configuration means somebody with dark lights can read what
/// the application thought it was doing, and can send it to somebody else.
///
/// The console layer stays for anyone running it from a terminal. The
/// returned guard has to stay alive for the process, or the last lines never
/// reach the disk.
fn start_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;
    // Brings with_filter onto the individual layers.
    use tracing_subscriber::Layer as _;

    let filter = || {
        tracing_subscriber::EnvFilter::try_from_env("MASLIGHT_LOG")
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
    };

    let dir = maslight_core::profile::config_dir();
    let file = std::fs::create_dir_all(&dir).ok().and_then(|()| {
        // Truncate rather than append: one launch is what somebody needs to
        // read, and an unbounded log on a machine that starts at login is a
        // problem of its own.
        std::fs::File::create(dir.join("maslight.log")).ok()
    });

    match file {
        Some(file) => {
            let (writer, guard) = tracing_appender::non_blocking(file);
            tracing_subscriber::registry()
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_ansi(false)
                        .with_writer(writer)
                        .with_filter(filter()),
                )
                .with(tracing_subscriber::fmt::layer().with_filter(filter()))
                .init();
            tracing::info!("MasLight {} starting", env!("CARGO_PKG_VERSION"));
            Some(guard)
        }
        None => {
            tracing_subscriber::fmt().with_env_filter(filter()).init();
            None
        }
    }
}

/// Entry point shared by the desktop binary and any future embedder.
pub fn run() {
    let _log = start_logging();

    let loaded = AppConfig::load_or_default();
    // Announce what happened here rather than inside the core crate, which has
    // no logging and no business deciding how a problem is reported.
    let where_from = maslight_core::profile::config_path();
    match &loaded {
        ConfigLoad::Loaded(c) => {
            tracing::info!(
                "configuration read from {} ({} bytes on disk, {} profile(s))",
                where_from.display(),
                std::fs::metadata(&where_from).map(|m| m.len()).unwrap_or(0),
                c.profiles.len()
            );
        }
        // Worth saying out loud. Somebody whose settings have apparently
        // vanished is usually looking at a different file than they think.
        ConfigLoad::Fresh(_) => {
            tracing::info!(
                "no configuration at {}, starting from defaults",
                where_from.display()
            );
        }
        ConfigLoad::Replaced { error, kept, .. } => {
            tracing::error!(
                "the configuration is not valid json ({error}); it has been kept as {} and MasLight is starting from defaults",
                kept.display()
            );
        }
        ConfigLoad::Unreadable { error, .. } => {
            tracing::error!(
                "could not read the configuration ({error}); starting from defaults and refusing to write over it"
            );
        }
    }
    // Never write over a configuration this process could not read. Defaults
    // carry no API token, and the block below would otherwise save them
    // straight over somebody's layout, calibration and devices.
    let may_write = loaded.writable();
    let mut config = loaded.into_config();

    // A configuration that asks for the API but carries no token would refuse
    // to start the server, so fill one in before anything reads it.
    if config.ui.api_token.trim().is_empty() {
        config.ui.api_token = maslight_api::generate_token();
        // Write it straight away: a token that only exists in memory would
        // change on every launch and break every script that stored it.
        if may_write {
            if let Err(e) = config.save_to(&maslight_core::profile::config_path()) {
                tracing::warn!("could not store the API token: {e}");
            }
        }
    }
    // One line that says what is about to run. Somebody reading this log
    // because their lights are dark should not have to guess which profile
    // was active or whether it had any LEDs in it.
    {
        let p = config.active();
        tracing::info!(
            "profile {:?} ({}), mode {:?}, {} leds, {} device(s), enabled {}",
            p.name,
            p.id,
            p.mode,
            p.layout.len(),
            p.devices.len(),
            config.enabled
        );
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
            discovery::calibration_plan,
            discovery::calibration_show,
            discovery::calibration_release,
            discovery::calibration_decode,
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
            // A missing tray is a degraded application, not a dead one.
            // Failing setup here would leave somebody with no window either,
            // and nothing on screen to explain why.
            if let Err(e) = build_tray(app.handle()) {
                tracing::error!("could not build the tray icon: {e}");
            }
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

    // The icon is whatever the bundle gave the window. If a build ever ships
    // without one, a tray with no picture is a much better outcome than the
    // whole application panicking on the way up.
    let mut tray = TrayIconBuilder::with_id("maslight");
    match app.default_window_icon().cloned() {
        Some(icon) => tray = tray.icon(icon),
        None => tracing::warn!("no window icon in this build, the tray will have none"),
    }

    tray.tooltip("MasLight")
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
