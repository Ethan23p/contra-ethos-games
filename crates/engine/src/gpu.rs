//! Device, queue and surface setup, shared by the windowed runner and headless rendering.

use std::sync::Arc;

use winit::window::Window;

use crate::shapes::ShapeRenderer;

pub(crate) async fn request_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> (wgpu::Adapter, wgpu::Device, wgpu::Queue) {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface,
            ..Default::default()
        })
        .await
        .expect("no WebGPU adapter");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("engine"),
            // Ask for exactly what this adapter offers, so phones aren't refused.
            required_limits: adapter.limits(),
            ..Default::default()
        })
        .await
        .expect("failed to create device");
    (adapter, device, queue)
}

/// Everything needed to draw into a window. Built asynchronously, because the
/// browser only hands out a GPU device through a promise.
pub(crate) struct Graphics {
    pub window: Arc<Window>,
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub shapes: ShapeRenderer,
}

impl Graphics {
    pub async fn new(window: Arc<Window>) -> Self {
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).expect("failed to create surface");
        let (adapter, device, queue) = request_device(&instance, Some(&surface)).await;

        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("surface not supported by adapter");
        // Non-sRGB everywhere, so colours blend the same on desktop and in the browser.
        let caps = surface.get_capabilities(&adapter);
        if let Some(format) = caps.formats.iter().copied().find(|f| !f.is_srgb()) {
            config.format = format;
        }
        surface.configure(&device, &config);

        let shapes = ShapeRenderer::new(&device, config.format);
        Self { window, surface, device, queue, config, shapes }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
}
