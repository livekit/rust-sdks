//! Direct3D 11 devices and textures for the D3D11 native buffer tests.

// Each test binary uses part of it.
#![allow(dead_code)]

use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Multithread, ID3D11Texture2D,
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_CPU_ACCESS_READ,
    D3D11_CPU_ACCESS_WRITE, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_CREATE_DEVICE_FLAG,
    D3D11_CREATE_DEVICE_VIDEO_SUPPORT, D3D11_SDK_VERSION, D3D11_SUBRESOURCE_DATA,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

/// A device as a capturer makes one: BGRA, video support when the driver
/// has it, multithread protection on.
pub struct Device {
    pub device: ID3D11Device,
    pub _context: ID3D11DeviceContext,
}

impl Device {
    pub fn new() -> Self {
        Self::with_flags(D3D11_CREATE_DEVICE_BGRA_SUPPORT | D3D11_CREATE_DEVICE_VIDEO_SUPPORT)
            .or_else(|| Self::with_flags(D3D11_CREATE_DEVICE_BGRA_SUPPORT))
            .expect("no Direct3D 11 device, not even WARP")
    }

    fn with_flags(flags: D3D11_CREATE_DEVICE_FLAG) -> Option<Self> {
        [D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP]
            .into_iter()
            .find_map(|driver| Self::create(driver, flags))
    }

    fn create(driver: D3D_DRIVER_TYPE, flags: D3D11_CREATE_DEVICE_FLAG) -> Option<Self> {
        let mut device = None;
        let mut context = None;
        unsafe {
            D3D11CreateDevice(
                None,
                driver,
                HMODULE::default(),
                flags,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .ok()?;
        }
        let device: ID3D11Device = device?;
        let context: ID3D11DeviceContext = context?;
        let multithread: ID3D11Multithread = context.cast().ok()?;
        unsafe {
            let _ = multithread.SetMultithreadProtected(true);
        }
        Some(Self { device, _context: context })
    }

    /// A BGRA texture filled from `pixel(x, y) -> [b, g, r, a]`, bound as
    /// a capturer's frame texture is.
    pub fn texture(
        &self,
        width: u32,
        height: u32,
        pixel: impl Fn(u32, u32) -> [u8; 4],
    ) -> ID3D11Texture2D {
        let binds = (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32;
        self.make_texture(width, height, D3D11_USAGE_DEFAULT, binds, 0, pixel)
    }

    /// The same picture in a staging texture: the video processor cannot
    /// take it as input, so the buffer has to read the BGRA back for the
    /// CPU.
    pub fn staging_texture(
        &self,
        width: u32,
        height: u32,
        pixel: impl Fn(u32, u32) -> [u8; 4],
    ) -> ID3D11Texture2D {
        let cpu = (D3D11_CPU_ACCESS_READ.0 | D3D11_CPU_ACCESS_WRITE.0) as u32;
        self.make_texture(width, height, D3D11_USAGE_STAGING, 0, cpu, pixel)
    }

    fn make_texture(
        &self,
        width: u32,
        height: u32,
        usage: D3D11_USAGE,
        bind_flags: u32,
        cpu_access: u32,
        pixel: impl Fn(u32, u32) -> [u8; 4],
    ) -> ID3D11Texture2D {
        let mut bytes = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                bytes.extend_from_slice(&pixel(x, y));
            }
        }
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: usage,
            BindFlags: bind_flags,
            CPUAccessFlags: cpu_access,
            MiscFlags: 0,
        };
        let data = D3D11_SUBRESOURCE_DATA {
            pSysMem: bytes.as_ptr() as *const _,
            SysMemPitch: width * 4,
            SysMemSlicePitch: 0,
        };
        let mut texture = None;
        unsafe { self.device.CreateTexture2D(&desc, Some(&data), Some(&mut texture)).unwrap() };
        texture.unwrap()
    }
}
