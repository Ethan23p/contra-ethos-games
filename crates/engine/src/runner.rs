//! The frame loop: one winit event loop that runs natively and in the browser.

use std::sync::Arc;

use glam::Vec2;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use crate::gpu::Graphics;
use crate::shapes::Shapes;

/// Timing and viewport for one frame.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Seconds since the app started.
    pub time: f32,
    /// Seconds since the previous frame.
    pub dt: f32,
    /// Viewport size in physical pixels.
    pub size: Vec2,
    /// Physical pixels per logical pixel (device pixel ratio).
    pub scale: f32,
}

pub trait App: 'static {
    /// Advance by one frame and describe what to draw.
    fn frame(&mut self, frame: &Frame, shapes: &mut Shapes);
}

pub fn run<A: App>(app: A) {
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        console_log::init_with_level(log::Level::Info).ok();
    }
    #[cfg(not(target_arch = "wasm32"))]
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn,wgpu_hal=error")).init();

    let event_loop = EventLoop::<Graphics>::with_user_event().build().expect("event loop");
    let runner = Runner {
        app,
        proxy: Some(event_loop.create_proxy()),
        graphics: None,
        shapes: Shapes::default(),
        start: Instant::now(),
        last: Instant::now(),
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut runner = runner;
        event_loop.run_app(&mut runner).expect("event loop failed");
    }
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(runner);
    }
}

struct Runner<A: App> {
    app: A,
    proxy: Option<EventLoopProxy<Graphics>>,
    graphics: Option<Graphics>,
    shapes: Shapes,
    start: Instant,
    last: Instant,
}

impl<A: App> ApplicationHandler<Graphics> for Runner<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Only the first resume creates the window; the proxy is spent then.
        let Some(proxy) = self.proxy.take() else { return };

        #[allow(unused_mut)]
        let mut attributes = Window::default_attributes().with_title("contra-ethos");
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;
            let canvas = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.get_element_by_id("canvas"))
                .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
                .expect("page needs <canvas id=\"canvas\">");
            attributes = attributes.with_canvas(Some(canvas));
        }
        let window = Arc::new(event_loop.create_window(attributes).expect("create window"));

        #[cfg(not(target_arch = "wasm32"))]
        {
            let graphics = pollster::block_on(Graphics::new(window));
            let _ = proxy.send_event(graphics);
        }
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let graphics = Graphics::new(window).await;
            let _ = proxy.send_event(graphics);
        });
    }

    fn user_event(&mut self, _: &ActiveEventLoop, graphics: Graphics) {
        graphics.window.request_redraw();
        self.graphics = Some(graphics);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(gfx) = self.graphics.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                gfx.resize(size.width, size.height);
                gfx.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let frame = Frame {
                    time: (now - self.start).as_secs_f32(),
                    dt: (now - self.last).as_secs_f32(),
                    size: Vec2::new(gfx.config.width as f32, gfx.config.height as f32),
                    scale: gfx.window.scale_factor() as f32,
                };
                self.last = now;

                self.shapes.clear_shapes();
                self.app.frame(&frame, &mut self.shapes);
                draw(gfx, &self.shapes, frame.size);
                gfx.window.request_redraw();
            }
            _ => {}
        }
    }
}

fn draw(gfx: &mut Graphics, shapes: &Shapes, size: Vec2) {
    use wgpu::CurrentSurfaceTexture as Current;
    let output = match gfx.surface.get_current_texture() {
        Current::Success(output) => output,
        Current::Suboptimal(output) => {
            gfx.surface.configure(&gfx.device, &gfx.config);
            output
        }
        Current::Outdated | Current::Lost => {
            // After a resize or tab switch: reconfigure and draw next frame.
            gfx.surface.configure(&gfx.device, &gfx.config);
            return;
        }
        other => {
            log::warn!("skipping frame: {other:?}");
            return;
        }
    };
    let view = output.texture.create_view(&Default::default());
    let mut encoder = gfx.device.create_command_encoder(&Default::default());
    gfx.shapes.render(&gfx.device, &gfx.queue, &mut encoder, &view, size, shapes);
    gfx.queue.submit([encoder.finish()]);
    gfx.window.pre_present_notify();
    gfx.queue.present(output);
}
