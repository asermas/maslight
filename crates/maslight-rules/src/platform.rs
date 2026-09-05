//! Gathering the facts rules ask about.
//!
//! Everything here is best effort. A probe that cannot answer returns the
//! quiet answer rather than an error: a rule that cannot be evaluated should
//! simply not fire, never break the lights.
//!
//! The sampler only asks for what the rule list actually needs. Enumerating
//! processes is the expensive probe, and most people never write a process
//! rule.

use maslight_core::AutoRule;

use crate::RuleContext;

/// Reads the system state a rule list needs.
#[derive(Debug, Default)]
pub struct Sampler {
    _private: (),
}

impl Sampler {
    pub fn sample(&mut self, rules: &[AutoRule]) -> RuleContext {
        let wants_processes = rules
            .iter()
            .any(|r| matches!(r, AutoRule::ProcessRunning { .. }));
        let wants_fullscreen = rules
            .iter()
            .any(|r| matches!(r, AutoRule::Fullscreen { .. }));
        let wants_battery = rules
            .iter()
            .any(|r| matches!(r, AutoRule::OnBattery { .. }));

        RuleContext {
            processes: if wants_processes {
                imp::processes()
            } else {
                Vec::new()
            },
            fullscreen: wants_fullscreen && imp::fullscreen(),
            minutes: local_minutes(),
            on_battery: wants_battery && imp::on_battery(),
        }
    }
}

/// Local time as minutes past midnight.
///
/// Worked out from the system clock and the platform offset rather than a date
/// library: this is the only date arithmetic in the project and it does not
/// justify a dependency.
fn local_minutes() -> u16 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let local = secs + imp::utc_offset_seconds();
    let day = local.rem_euclid(86_400);
    (day / 60) as u16
}

#[cfg(target_os = "windows")]
mod imp {
    use windows::Win32::Foundation::{CloseHandle, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Power::GetSystemPowerStatus;
    use windows::Win32::System::Time::GetTimeZoneInformation;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetDesktopWindow, GetForegroundWindow, GetShellWindow, GetWindowRect,
    };

    pub fn processes() -> Vec<String> {
        let mut out = Vec::new();
        // SAFETY: the snapshot handle is closed on every path.
        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return out;
            };
            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let end = entry
                        .szExeFile
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    out.push(String::from_utf16_lossy(&entry.szExeFile[..end]).to_lowercase());
                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }
            let _ = CloseHandle(snapshot);
        }
        out
    }

    pub fn fullscreen() -> bool {
        // SAFETY: plain window queries on handles the system owns.
        unsafe {
            let window = GetForegroundWindow();
            if window.is_invalid() || window == GetShellWindow() || window == GetDesktopWindow() {
                return false;
            }
            let mut rect = RECT::default();
            if GetWindowRect(window, &mut rect).is_err() {
                return false;
            }
            let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return false;
            }
            let m = info.rcMonitor;
            // A window counts as fullscreen when it covers the monitor. The
            // slack absorbs borderless windows that overshoot by a pixel.
            rect.left <= m.left + 1
                && rect.top <= m.top + 1
                && rect.right >= m.right - 1
                && rect.bottom >= m.bottom - 1
        }
    }

    pub fn on_battery() -> bool {
        let mut status = Default::default();
        // SAFETY: writes into a struct we own.
        unsafe {
            if GetSystemPowerStatus(&mut status).is_err() {
                return false;
            }
        }
        // 0 means offline, which is to say running from the battery.
        status.ACLineStatus == 0
    }

    pub fn utc_offset_seconds() -> i64 {
        let mut info = Default::default();
        // SAFETY: writes into a struct we own.
        let kind = unsafe { GetTimeZoneInformation(&mut info) };
        // Bias is minutes to add to local time to reach UTC, so the offset the
        // other way is its negation. Daylight saving adds its own bias.
        let extra = match kind {
            2 => info.DaylightBias, // TIME_ZONE_ID_DAYLIGHT
            1 => info.StandardBias, // TIME_ZONE_ID_STANDARD
            _ => 0,
        };
        -((info.Bias + extra) as i64) * 60
    }
}

#[cfg(not(target_os = "windows"))]
mod imp {
    pub fn processes() -> Vec<String> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return out;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.bytes().all(|b| b.is_ascii_digit()) {
                continue;
            }
            if let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) {
                out.push(comm.trim().to_lowercase());
            }
        }
        out
    }

    pub fn fullscreen() -> bool {
        // Wayland deliberately does not let one client ask about another, and
        // an X11 probe here would mean a second connection for a rule most
        // people will not write. Left to a later release rather than answered
        // wrongly.
        false
    }

    pub fn on_battery() -> bool {
        let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else {
            return false;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let kind = std::fs::read_to_string(path.join("type")).unwrap_or_default();
            if kind.trim() != "Mains" {
                continue;
            }
            if let Ok(online) = std::fs::read_to_string(path.join("online")) {
                return online.trim() == "0";
            }
        }
        false
    }

    pub fn utc_offset_seconds() -> i64 {
        // `date` knows the zone rules; parsing its answer is cheaper than
        // carrying a time zone database for one rule type.
        std::process::Command::new("date")
            .arg("+%z")
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok())
            .and_then(|s| parse_offset(s.trim()))
            .unwrap_or(0)
    }

    /// Parse `+0300` into seconds.
    fn parse_offset(value: &str) -> Option<i64> {
        if value.len() < 5 {
            return None;
        }
        let sign = if value.starts_with('-') { -1 } else { 1 };
        let hours: i64 = value[1..3].parse().ok()?;
        let minutes: i64 = value[3..5].parse().ok()?;
        Some(sign * (hours * 3600 + minutes * 60))
    }
}
