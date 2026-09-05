//! Starting with the session.
//!
//! Done by hand rather than with a plugin: on Windows it is one registry
//! value, on Linux it is one desktop entry, and both are things a user should
//! be able to inspect and delete without the app.

/// True when this build can manage the login item itself.
pub fn supported() -> bool {
    cfg!(any(target_os = "windows", target_os = "linux"))
}

#[cfg(target_os = "windows")]
mod imp {
    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const VALUE_NAME: &str = "MasLight";

    pub fn set(enabled: bool) -> Result<(), String> {
        let key = windows_registry::CURRENT_USER
            .create(RUN_KEY)
            .map_err(|e| e.to_string())?;
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            // `--tray` tells the app to come up hidden.
            let command = format!("\"{}\" --tray", exe.display());
            key.set_string(VALUE_NAME, &command)
                .map_err(|e| e.to_string())?;
        } else {
            // Removing a value that is not there is not an error for us.
            let _ = key.remove_value(VALUE_NAME);
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use std::path::PathBuf;

    pub fn set(enabled: bool) -> Result<(), String> {
        let file = autostart_dir().join("maslight.desktop");
        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            std::fs::create_dir_all(autostart_dir()).map_err(|e| e.to_string())?;
            let entry = format!(
                "[Desktop Entry]\nType=Application\nName=MasLight\nExec={} --tray\nIcon=maslight\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
                exe.display()
            );
            std::fs::write(&file, entry).map_err(|e| e.to_string())?;
        } else if file.exists() {
            std::fs::remove_file(&file).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    fn autostart_dir() -> PathBuf {
        let base = match std::env::var("XDG_CONFIG_HOME") {
            Ok(xdg) if !xdg.is_empty() => PathBuf::from(xdg),
            _ => match std::env::var("HOME") {
                Ok(home) => PathBuf::from(home).join(".config"),
                Err(_) => PathBuf::from(".config"),
            },
        };
        base.join("autostart")
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod imp {
    pub fn set(_enabled: bool) -> Result<(), String> {
        Err(String::from(
            "starting at login is not implemented on this platform yet",
        ))
    }
}

/// Turn the login item on or off.
pub fn set(enabled: bool) -> Result<(), String> {
    imp::set(enabled)
}
