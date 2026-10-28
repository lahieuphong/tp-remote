//! Direct3D 11 device and the CPU readback used by the local preview.
//!
//! One device per capture run, created on the default hardware adapter with BGRA
//! support and multithread protection (WGC uses the device from its own threads). The
//! readback copies only the frame's valid content into a reused staging texture, maps it
//! and converts it with [`crate::scale`]. Nothing is written to disk.

use std::sync::{Mutex, PoisonError};

use tp_contracts::capture::PixelSize;
use tp_contracts::error::{CoreError, ErrorCategory};
use windows::Graphics::DirectX::Direct3D11::{IDirect3DDevice, IDirect3DSurface};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BOX, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::core::Interface;

use super::os_error;
use crate::scale;

pub(crate) const DEVICE_LOST: CoreError = CoreError::new(
    ErrorCategory::PlatformUnavailable,
    "the graphics device was removed or reset",
);

struct Staging {
    texture: ID3D11Texture2D,
    width: u32,
    height: u32,
}

pub(crate) struct Gpu {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    adapter: String,
    staging: Mutex<Option<Staging>>,
}

impl Gpu {
    pub fn create() -> Result<Gpu, CoreError> {
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        // SAFETY: the out-pointers are valid locals; default adapter, hardware driver,
        // no software module, default feature levels.
        unsafe {
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
        }
        .map_err(os_error(
            ErrorCategory::PlatformUnavailable,
            "D3D11CreateDevice failed",
        ))?;
        let missing = || CoreError::new(ErrorCategory::InternalError, "D3D11 returned no device");
        let device = device.ok_or_else(missing)?;
        let context = context.ok_or_else(missing)?;
        if let Ok(multithread) = device.cast::<ID3D11Multithread>() {
            // SAFETY: plain setter on a valid interface.
            let _ = unsafe { multithread.SetMultithreadProtected(true) };
        }
        let adapter = adapter_name(&device).unwrap_or_else(|| "unknown".to_owned());
        Ok(Gpu {
            device,
            context,
            adapter,
            staging: Mutex::new(None),
        })
    }

    /// Sanitized adapter description, e.g. `NVIDIA GeForce RTX 3090`.
    pub fn adapter(&self) -> &str {
        &self.adapter
    }

    /// The WinRT wrapper of the device, as WGC's frame pool requires.
    pub fn winrt_device(&self) -> Result<IDirect3DDevice, CoreError> {
        let error = || {
            os_error(
                ErrorCategory::PlatformUnavailable,
                "WinRT D3D device failed",
            )
        };
        let dxgi: IDXGIDevice = self.device.cast().map_err(error())?;
        // SAFETY: `dxgi` is a valid DXGI device; the call returns a new reference.
        let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }.map_err(error())?;
        inspectable.cast().map_err(error())
    }

    pub fn is_lost(&self) -> bool {
        // SAFETY: plain query on a valid device.
        unsafe { self.device.GetDeviceRemovedReason() }.is_err()
    }

    /// Copies the valid `content` region of `surface` to the CPU and appends it to `out`
    /// as RGBA8, scaled to fit within `max`.
    pub fn read_rgba(
        &self,
        surface: &IDirect3DSurface,
        content: PixelSize,
        max: PixelSize,
        out: &mut Vec<u8>,
    ) -> Result<PixelSize, CoreError> {
        let access: IDirect3DDxgiInterfaceAccess = surface.cast().map_err(os_error(
            ErrorCategory::InternalError,
            "surface has no DXGI access",
        ))?;
        // SAFETY: GetInterface returns a new reference to the surface's texture.
        let texture: ID3D11Texture2D = unsafe { access.GetInterface() }.map_err(os_error(
            ErrorCategory::InternalError,
            "surface is not a D3D11 texture",
        ))?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: `desc` is a valid out-pointer.
        unsafe { texture.GetDesc(&mut desc) };
        if desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err(CoreError::new(
                ErrorCategory::UnsupportedCapability,
                "frame is not BGRA8",
            ));
        }
        let size = PixelSize {
            width: content.width.min(desc.Width),
            height: content.height.min(desc.Height),
        };
        if size.width == 0 || size.height == 0 {
            return Err(CoreError::new(ErrorCategory::InvalidPayload, "empty frame"));
        }
        let dst = scale::fit_within(size, max);
        if dst.width == 0 || dst.height == 0 {
            return Err(CoreError::new(
                ErrorCategory::InvalidPayload,
                "empty preview size",
            ));
        }

        let mut staging = self.staging.lock().unwrap_or_else(PoisonError::into_inner);
        if !staging
            .as_ref()
            .is_some_and(|s| s.width == size.width && s.height == size.height)
        {
            *staging = Some(self.create_staging(size)?);
        }
        let Some(staging) = staging.as_ref() else {
            return Err(CoreError::new(
                ErrorCategory::InternalError,
                "no staging texture",
            ));
        };

        let region = D3D11_BOX {
            left: 0,
            top: 0,
            front: 0,
            right: size.width,
            bottom: size.height,
            back: 1,
        };
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: both textures belong to `self.device`; the region lies within the
        // source (clamped to its description) and matches the staging size. The
        // immediate context is only used here, under the `staging` lock.
        let map = unsafe {
            self.context.CopySubresourceRegion(
                &staging.texture,
                0,
                0,
                0,
                0,
                &texture,
                0,
                Some(&region),
            );
            self.context
                .Map(&staging.texture, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
        };
        if map.is_err() {
            return Err(if self.is_lost() {
                DEVICE_LOST
            } else {
                CoreError::new(
                    ErrorCategory::InternalError,
                    "mapping the staging texture failed",
                )
            });
        }
        let stride = mapped.RowPitch as usize;
        let len = stride * (size.height as usize - 1) + size.width as usize * 4;
        let result = if mapped.pData.is_null() || stride < size.width as usize * 4 {
            Err(CoreError::new(
                ErrorCategory::InternalError,
                "unexpected mapping layout",
            ))
        } else {
            // SAFETY: Map succeeded, so `pData` points to subresource 0 of the staging
            // texture: `height` rows of `RowPitch` bytes, readable until Unmap. The slice
            // covers exactly the last row's used bytes and is dropped before Unmap.
            let pixels = unsafe { std::slice::from_raw_parts(mapped.pData as *const u8, len) };
            scale::bgra_to_rgba_scaled(pixels, size, stride, dst, out).map_err(|_| {
                CoreError::new(ErrorCategory::InternalError, "preview conversion failed")
            })
        };
        // SAFETY: the texture was mapped above; no slice into it is alive any more.
        unsafe { self.context.Unmap(&staging.texture, 0) };
        result.map(|()| dst)
    }

    fn create_staging(&self, size: PixelSize) -> Result<Staging, CoreError> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: size.width,
            Height: size.height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };
        let mut texture: Option<ID3D11Texture2D> = None;
        // SAFETY: valid description and out-pointer; no initial data.
        unsafe { self.device.CreateTexture2D(&desc, None, Some(&mut texture)) }.map_err(|e| {
            if self.is_lost() {
                DEVICE_LOST
            } else {
                os_error(
                    ErrorCategory::InternalError,
                    "creating the staging texture failed",
                )(e)
            }
        })?;
        let texture = texture
            .ok_or_else(|| CoreError::new(ErrorCategory::InternalError, "no staging texture"))?;
        Ok(Staging {
            texture,
            width: size.width,
            height: size.height,
        })
    }
}

fn adapter_name(device: &ID3D11Device) -> Option<String> {
    let dxgi: IDXGIDevice = device.cast().ok()?;
    // SAFETY: plain queries on valid interfaces.
    let desc = unsafe { dxgi.GetAdapter().ok()?.GetDesc().ok()? };
    let end = desc
        .Description
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(desc.Description.len());
    let name: String = String::from_utf16_lossy(&desc.Description[..end])
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    Some(name.trim().to_owned())
}
