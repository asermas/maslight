//! Windows capture through DXGI Desktop Duplication.
//!
//! The naive way to use Desktop Duplication is to copy the whole desktop into
//! a staging texture and map it. At 2560x1440x60 that is 880 MB/s crossing the
//! bus and a lot of it lands on the CPU.
//!
//! MasLight copies the desktop into a texture that owns a full mip chain,
//! asks the GPU to generate the mips, and then reads back a *small* mip. The
//! box filter that produces the mip is exactly the average the reducer wants,
//! so quality goes up while the readback drops by two orders of magnitude: a
//! 2560x1440 desktop is read back as 320x180, which is 230 KB/s at 60 fps.

use std::time::Duration;

use maslight_core::{CaptureBackendKind, CaptureSettings, FrameView, PixelFormat};
use windows::core::Interface;
use windows::Win32::Foundation::{E_ACCESSDENIED, HMODULE};
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView,
    ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
    D3D11_RESOURCE_MISC_GENERATE_MIPS, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{
    IDXGIAdapter, IDXGIDevice, IDXGIOutput1, IDXGIOutputDuplication, IDXGIResource,
    DXGI_ERROR_ACCESS_LOST, DXGI_ERROR_NOT_FOUND, DXGI_ERROR_WAIT_TIMEOUT, DXGI_OUTDUPL_FRAME_INFO,
    DXGI_OUTPUT_DESC,
};

use crate::{CaptureBackend, CaptureError, DisplayInfo, FrameStatus};

/// Longest side of the image we read back. Everything above this is averaged
/// away on the GPU for free.
const TARGET_READBACK_WIDTH: u32 = 320;

pub struct DxgiBackend {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    session: Option<Session>,
}

struct Session {
    duplication: IDXGIOutputDuplication,
    display_id: String,
    src: (u32, u32),
    small: (u32, u32),
    mip_level: u32,
    format: DXGI_FORMAT,
    mip_texture: ID3D11Texture2D,
    mip_view: ID3D11ShaderResourceView,
    staging: ID3D11Texture2D,
    holding_frame: bool,
}

impl DxgiBackend {
    pub fn new() -> Result<Self, CaptureError> {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        let levels = [D3D_FEATURE_LEVEL_11_0];
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                Some(&levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(|e| CaptureError::Platform(format!("D3D11CreateDevice failed: {e}")))?;
        }
        let device = device.ok_or_else(|| CaptureError::Platform("no D3D11 device".into()))?;
        let context = context.ok_or_else(|| CaptureError::Platform("no D3D11 context".into()))?;
        Ok(Self {
            device,
            context,
            session: None,
        })
    }

    fn adapter(&self) -> Result<IDXGIAdapter, CaptureError> {
        let dxgi: IDXGIDevice = self
            .device
            .cast()
            .map_err(|e| CaptureError::Platform(format!("no IDXGIDevice: {e}")))?;
        unsafe { dxgi.GetAdapter() }.map_err(|e| CaptureError::Platform(format!("no adapter: {e}")))
    }

    fn outputs(&self) -> Result<Vec<(IDXGIOutput1, DXGI_OUTPUT_DESC)>, CaptureError> {
        let adapter = self.adapter()?;
        let mut out = Vec::new();
        let mut index = 0u32;
        loop {
            let output = match unsafe { adapter.EnumOutputs(index) } {
                Ok(o) => o,
                Err(e) if e.code() == DXGI_ERROR_NOT_FOUND => break,
                Err(e) => return Err(CaptureError::Platform(format!("EnumOutputs: {e}"))),
            };
            index += 1;
            let desc = unsafe { output.GetDesc() }
                .map_err(|e| CaptureError::Platform(format!("GetDesc: {e}")))?;
            if !desc.AttachedToDesktop.as_bool() {
                continue;
            }
            let output1: IDXGIOutput1 = output
                .cast()
                .map_err(|e| CaptureError::Platform(format!("no IDXGIOutput1: {e}")))?;
            out.push((output1, desc));
        }
        Ok(out)
    }

    /// Build the mip texture, its view and the small staging texture.
    #[allow(clippy::type_complexity)]
    fn build_surfaces(
        &self,
        width: u32,
        height: u32,
        format: DXGI_FORMAT,
    ) -> Result<
        (
            ID3D11Texture2D,
            ID3D11ShaderResourceView,
            ID3D11Texture2D,
            u32,
            (u32, u32),
        ),
        CaptureError,
    > {
        // Pick the mip level whose width first drops to the readback target.
        let mut mip_level = 0u32;
        while (width >> mip_level) > TARGET_READBACK_WIDTH && (height >> mip_level) > 2 {
            mip_level += 1;
        }
        let small = ((width >> mip_level).max(1), (height >> mip_level).max(1));

        let mip_desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            // 0 asks D3D11 for the complete chain down to 1x1.
            MipLevels: 0,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
        };
        let mut mip_texture: Option<ID3D11Texture2D> = None;
        unsafe {
            self.device
                .CreateTexture2D(&mip_desc, None, Some(&mut mip_texture))
                .map_err(|e| CaptureError::Platform(format!("mip texture: {e}")))?;
        }
        let mip_texture =
            mip_texture.ok_or_else(|| CaptureError::Platform("mip texture missing".into()))?;

        let mut view: Option<ID3D11ShaderResourceView> = None;
        unsafe {
            self.device
                .CreateShaderResourceView(&mip_texture, None, Some(&mut view))
                .map_err(|e| CaptureError::Platform(format!("mip view: {e}")))?;
        }
        let view = view.ok_or_else(|| CaptureError::Platform("mip view missing".into()))?;

        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: small.0,
            Height: small.1,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut staging: Option<ID3D11Texture2D> = None;
        unsafe {
            self.device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging))
                .map_err(|e| CaptureError::Platform(format!("staging texture: {e}")))?;
        }
        let staging =
            staging.ok_or_else(|| CaptureError::Platform("staging texture missing".into()))?;

        Ok((mip_texture, view, staging, mip_level, small))
    }

    fn release_held(&mut self) {
        if let Some(s) = &mut self.session {
            if s.holding_frame {
                let _ = unsafe { s.duplication.ReleaseFrame() };
                s.holding_frame = false;
            }
        }
    }
}

fn device_name(desc: &DXGI_OUTPUT_DESC) -> String {
    let raw = &desc.DeviceName;
    let end = raw.iter().position(|c| *c == 0).unwrap_or(raw.len());
    String::from_utf16_lossy(&raw[..end])
}

impl CaptureBackend for DxgiBackend {
    fn kind(&self) -> CaptureBackendKind {
        CaptureBackendKind::Dxgi
    }

    fn displays(&self) -> Result<Vec<DisplayInfo>, CaptureError> {
        let mut out = Vec::new();
        for (i, (_, desc)) in self.outputs()?.into_iter().enumerate() {
            let r = desc.DesktopCoordinates;
            out.push(DisplayInfo {
                id: device_name(&desc),
                label: format!("Display {}", i + 1),
                width: (r.right - r.left).max(0) as u32,
                height: (r.bottom - r.top).max(0) as u32,
                x: r.left,
                y: r.top,
                // The primary display is the one whose top-left is the origin.
                primary: r.left == 0 && r.top == 0,
            });
        }
        if out.is_empty() {
            return Err(CaptureError::NoBackend);
        }
        Ok(out)
    }

    fn start(
        &mut self,
        display: Option<&str>,
        _settings: &CaptureSettings,
    ) -> Result<(), CaptureError> {
        self.stop();

        let outputs = self.outputs()?;
        let (output, desc) = match display {
            Some(id) => outputs
                .into_iter()
                .find(|(_, d)| device_name(d) == id)
                .ok_or_else(|| CaptureError::NoSuchDisplay(id.to_string()))?,
            None => outputs
                .into_iter()
                .find(|(_, d)| d.DesktopCoordinates.left == 0 && d.DesktopCoordinates.top == 0)
                .ok_or(CaptureError::NoBackend)?,
        };

        let duplication = unsafe { output.DuplicateOutput(&self.device) }.map_err(|e| {
            if e.code() == E_ACCESSDENIED {
                // A fullscreen exclusive game or a secure desktop such as the
                // UAC prompt owns the output. Retrying later is the fix.
                CaptureError::PermissionDenied
            } else {
                CaptureError::Platform(format!("DuplicateOutput: {e}"))
            }
        })?;

        let dupl_desc = unsafe { duplication.GetDesc() };
        let width = dupl_desc.ModeDesc.Width.max(1);
        let height = dupl_desc.ModeDesc.Height.max(1);
        let format = if dupl_desc.ModeDesc.Format == DXGI_FORMAT_R16G16B16A16_FLOAT {
            DXGI_FORMAT_R16G16B16A16_FLOAT
        } else {
            DXGI_FORMAT_B8G8R8A8_UNORM
        };

        let (mip_texture, mip_view, staging, mip_level, small) =
            self.build_surfaces(width, height, format)?;

        tracing::info!(
            "DXGI capture on {} at {}x{}, reading back {}x{} from mip {}",
            device_name(&desc),
            width,
            height,
            small.0,
            small.1,
            mip_level
        );

        self.session = Some(Session {
            duplication,
            display_id: device_name(&desc),
            src: (width, height),
            small,
            mip_level,
            format,
            mip_texture,
            mip_view,
            staging,
            holding_frame: false,
        });
        Ok(())
    }

    fn next_frame(
        &mut self,
        timeout: Duration,
        on_frame: &mut dyn FnMut(&FrameView<'_>),
    ) -> Result<FrameStatus, CaptureError> {
        self.release_held();
        let context = self.context.clone();
        let session = self.session.as_mut().ok_or(CaptureError::NotStarted)?;

        let mut info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut resource: Option<IDXGIResource> = None;
        let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u32;

        match unsafe {
            session
                .duplication
                .AcquireNextFrame(timeout_ms, &mut info, &mut resource)
        } {
            Ok(()) => {}
            Err(e) if e.code() == DXGI_ERROR_WAIT_TIMEOUT => return Ok(FrameStatus::Timeout),
            Err(e) if e.code() == DXGI_ERROR_ACCESS_LOST => return Ok(FrameStatus::Lost),
            Err(e) => return Err(CaptureError::Platform(format!("AcquireNextFrame: {e}"))),
        }
        session.holding_frame = true;

        // A zero present time means only the pointer moved. The desktop image
        // is unchanged, so there is nothing to recompute.
        if info.LastPresentTime == 0 {
            let _ = unsafe { session.duplication.ReleaseFrame() };
            session.holding_frame = false;
            return Ok(FrameStatus::Unchanged);
        }

        let Some(resource) = resource else {
            return Ok(FrameStatus::Unchanged);
        };
        let desktop: ID3D11Texture2D = resource
            .cast()
            .map_err(|e| CaptureError::Platform(format!("frame is not a texture: {e}")))?;

        let status = unsafe {
            // Desktop -> mip 0, then let the GPU build the pyramid.
            context.CopySubresourceRegion(&session.mip_texture, 0, 0, 0, 0, &desktop, 0, None);
            context.GenerateMips(&session.mip_view);
            // One small mip -> a staging surface the CPU can map.
            context.CopySubresourceRegion(
                &session.staging,
                0,
                0,
                0,
                0,
                &session.mip_texture,
                session.mip_level,
                None,
            );

            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context
                .Map(&session.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                .map_err(|e| CaptureError::Platform(format!("Map: {e}")))?;

            let stride = mapped.RowPitch as usize;
            let len = stride * session.small.1 as usize;
            let data = std::slice::from_raw_parts(mapped.pData as *const u8, len);
            let format = if session.format == DXGI_FORMAT_R16G16B16A16_FLOAT {
                PixelFormat::Rgba16FScrgb
            } else {
                PixelFormat::Bgra8
            };
            let view = FrameView::new(data, session.small.0, session.small.1, stride, format);
            on_frame(&view);
            context.Unmap(&session.staging, 0);
            FrameStatus::New
        };

        let _ = unsafe { session.duplication.ReleaseFrame() };
        session.holding_frame = false;
        Ok(status)
    }

    fn stop(&mut self) {
        self.release_held();
        self.session = None;
    }

    fn current_size(&self) -> Option<(u32, u32)> {
        self.session.as_ref().map(|s| s.src)
    }
}

impl DxgiBackend {
    /// Display this session is bound to, if any.
    pub fn current_display(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.display_id.as_str())
    }
}

// Desktop Duplication objects are apartment-agnostic COM objects and the
// engine owns the backend from a single worker thread.
unsafe impl Send for DxgiBackend {}
