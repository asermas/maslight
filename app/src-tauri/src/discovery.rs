//! Commands for camera assisted position discovery.
//!
//! The photographs are decoded by the webview, which already has a JPEG and
//! PNG decoder, and arrive here as base64 greyscale planes. That keeps an
//! image decoding dependency out of the application entirely and sends a
//! quarter of the bytes a base64 JPEG would.

use maslight_calibrate::{build_layout, discover, plan, DetectSettings, Frame, Homography, Step};
use maslight_core::Layout;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

/// One step of the sequence, described for the interface.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanStep {
    pub index: usize,
    /// `off`, `on` or `bit`.
    pub kind: String,
    /// Which bit, for a bit step.
    pub bit: Option<u32>,
}

#[tauri::command]
pub fn calibration_plan(led_count: usize) -> Vec<PlanStep> {
    plan(led_count.clamp(1, 4096))
        .into_iter()
        .enumerate()
        .map(|(index, step)| match step {
            Step::AllOff => PlanStep {
                index,
                kind: String::from("off"),
                bit: None,
            },
            Step::AllOn => PlanStep {
                index,
                kind: String::from("on"),
                bit: None,
            },
            Step::Bit(bit) => PlanStep {
                index,
                kind: String::from("bit"),
                bit: Some(bit),
            },
        })
        .collect()
}

/// Light the pattern for one step so it can be photographed.
#[tauri::command]
pub fn calibration_show(state: State<'_, AppState>, led_count: usize, step: usize) {
    let steps = plan(led_count.clamp(1, 4096));
    let Some(step) = steps.get(step) else {
        return;
    };
    let mask: Vec<bool> = (0..led_count).map(|i| step.lights(i)).collect();
    state.shared.engine.lock().show_pattern(Some(mask));
}

/// Release the strip when the sequence is finished or abandoned.
#[tauri::command]
pub fn calibration_release(state: State<'_, AppState>) {
    state.shared.engine.lock().show_pattern(None);
}

/// A photograph, already reduced to a greyscale plane by the webview.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    /// Base64 of one byte per pixel, row major.
    pub luma: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodeRequest {
    pub led_count: usize,
    pub photos: Vec<Photo>,
    /// Screen corners in the photograph: top left, top right, bottom right,
    /// bottom left, in pixels.
    pub corners: [[f32; 2]; 4],
    pub display: String,
    pub depth: Option<f32>,
    pub threshold: Option<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodeResult {
    pub layout: Layout,
    pub found: usize,
    pub expected: usize,
    /// Spots that were seen but could not be numbered, usually reflections.
    pub unassigned: usize,
}

#[tauri::command]
pub fn calibration_decode(request: DecodeRequest) -> Result<DecodeResult, String> {
    let frames: Vec<Frame> = request
        .photos
        .iter()
        .map(|photo| {
            let luma = decode_base64(&photo.luma)?;
            let expected = photo.width as usize * photo.height as usize;
            if luma.len() < expected {
                return Err(format!(
                    "a photograph is {} bytes, expected {expected}",
                    luma.len()
                ));
            }
            Ok(Frame::new(luma, photo.width, photo.height))
        })
        .collect::<Result<_, String>>()?;

    let settings = DetectSettings {
        threshold: request.threshold.unwrap_or(40),
        ..Default::default()
    };
    let discovery = discover(request.led_count, &frames, &settings).map_err(|e| e.to_string())?;

    let corners = [
        (request.corners[0][0], request.corners[0][1]),
        (request.corners[1][0], request.corners[1][1]),
        (request.corners[2][0], request.corners[2][1]),
        (request.corners[3][0], request.corners[3][1]),
    ];
    let screen = Homography::from_screen_corners(corners)
        .ok_or_else(|| String::from("those four corners do not describe a screen"))?;

    let layout = build_layout(
        &discovery,
        &screen,
        &request.display,
        request.depth.unwrap_or(0.12),
    );

    Ok(DecodeResult {
        layout,
        found: discovery.found,
        expected: discovery.expected,
        unassigned: discovery.unassigned.len(),
    })
}

/// Decode standard base64.
///
/// Written out rather than pulled in: it is twenty lines, it runs once per
/// photograph, and it is one less dependency in the application.
fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\n' | b'\r' => continue,
            _ => return Err(String::from("the photograph is not valid base64")),
        } as u32;
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}
