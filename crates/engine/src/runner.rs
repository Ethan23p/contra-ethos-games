//! The frame loop: one winit event loop that runs natively and in the browser.
//!
//! Each frame: deliver input, run the simulation in fixed steps, then draw with
//! [`Frame::alpha`] saying how far the clock is between the last two steps, so
//! motion can be interpolated and stays smooth at any display rate.

use std::sync::Arc;

use glam::Vec2;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::gpu::Graphics;
use crate::input::{GestureConfig, Gestures, Input, Phase, Pointer};
use crate::shapes::{Shapes, View, rgba};

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
    /// Fraction of a step the clock has run past the last [`App::update`], in
    /// [0, 1): draw at `lerp(previous, current, alpha)`.
    pub alpha: f32,
}

pub trait App: 'static {
    /// Length of one simulation step, seconds.
    fn step(&self) -> f32 {
        1.0 / 120.0
    }

    fn gestures(&self) -> GestureConfig {
        GestureConfig::default()
    }

    /// Called for each input as it arrives, before the frame's updates.
    fn input(&mut self, _input: Input, _frame: &Frame) {}

    /// Advance the simulation by exactly one step of `dt` seconds.
    fn update(&mut self, _dt: f32) {}

    /// Describe what to draw.
    fn frame(&mut self, frame: &Frame, shapes: &mut Shapes);

    /// Contents of the dev panel (egui), shown while the panel is open.
    fn dev_ui(&mut self, _ui: &mut egui::Ui) {}
}

/// The longest stretch one frame may simulate, so a stall (a breakpoint, a tab
/// in the background) doesn't turn into a burst of thousands of steps.
const MAX_CATCH_UP: f32 = 0.25;

/// Size of the corner hotspot that opens the dev panel, logical pixels.
const DEV_HOTSPOT: f32 = 44.0;

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
        dev: None,
        shapes: Shapes::default(),
        gestures: Gestures::default(),
        inputs: Vec::new(),
        owned: Vec::new(),
        mouse: Vec2::ZERO,
        mouse_down: false,
        start: Instant::now(),
        last: Instant::now(),
        accumulator: 0.0,
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

struct Dev {
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    open: bool,
}

struct Runner<A: App> {
    app: A,
    proxy: Option<EventLoopProxy<Graphics>>,
    graphics: Option<Graphics>,
    dev: Option<Dev>,
    shapes: Shapes,
    gestures: Gestures,
    inputs: Vec<Input>,
    /// Pointers that went down on the game, not the panel; only these reach it.
    owned: Vec<u64>,
    mouse: Vec2,
    mouse_down: bool,
    start: Instant,
    last: Instant,
    accumulator: f32,
}

impl<A: App> Runner<A> {
    fn now(&self) -> f32 {
        self.start.elapsed().as_secs_f32()
    }

    fn frame_info(&self, dt: f32) -> Frame {
        let gfx = self.graphics.as_ref().expect("graphics");
        Frame {
            time: self.now(),
            dt,
            size: Vec2::new(gfx.config.width as f32, gfx.config.height as f32),
            scale: gfx.window.scale_factor() as f32,
            alpha: self.accumulator / self.app.step(),
        }
    }

    fn pointer(&mut self, p: Pointer, panel_took_it: bool) {
        let frame = self.frame_info(0.0);
        if p.phase == Phase::Down {
            if panel_took_it {
                return;
            }
            let corner = Vec2::new(frame.size.x - p.pos.x, p.pos.y);
            if corner.max_element() < DEV_HOTSPOT * frame.scale {
                if let Some(dev) = self.dev.as_mut() {
                    dev.open = !dev.open;
                }
                return;
            }
            self.owned.push(p.id);
        } else if !self.owned.contains(&p.id) {
            // Hover moves from the mouse still reach the game, unless over the panel.
            if !(p.id == 0 && p.phase == Phase::Move && !panel_took_it) {
                return;
            }
        }
        if matches!(p.phase, Phase::Up | Phase::Cancel) {
            self.owned.retain(|&id| id != p.id);
        }

        self.gestures.pointer(p, self.now(), frame.scale, self.app.gestures(), &mut self.inputs);
        self.deliver(&frame);
    }

    fn deliver(&mut self, frame: &Frame) {
        for input in self.inputs.drain(..) {
            self.app.input(input, frame);
        }
    }

    fn redraw(&mut self) {
        // Resize events can be missed or arrive late on the web (the canvas is
        // sized by CSS), so every frame matches the surface to the window.
        if let Some(gfx) = self.graphics.as_mut() {
            let size = gfx.window.inner_size();
            if (size.width, size.height) != (gfx.config.width, gfx.config.height) {
                gfx.resize(size.width, size.height);
            }
        }

        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32();
        self.last = now;

        self.gestures.tick(self.now(), self.app.gestures(), &mut self.inputs);
        let frame = self.frame_info(dt);
        self.deliver(&frame);

        let step = self.app.step();
        self.accumulator += dt.min(MAX_CATCH_UP);
        while self.accumulator >= step {
            self.app.update(step);
            self.accumulator -= step;
        }

        let frame = self.frame_info(dt);
        self.shapes.clear_shapes();
        self.shapes.view = View::PIXELS;
        self.app.frame(&frame, &mut self.shapes);
        let open = self.dev.as_ref().is_some_and(|d| d.open);
        draw_dev_hotspot(&mut self.shapes, &frame, open);

        let gfx = self.graphics.as_mut().expect("graphics");
        let dev = self.dev.as_mut().expect("dev panel");
        let app = &mut self.app;
        draw(gfx, dev, &self.shapes, frame.size, |ui| {
            egui::Panel::right("dev")
                .default_size((frame.size.x / frame.scale * 0.85).min(360.0))
                .frame(egui::Frame::side_top_panel(ui.style()).fill(egui::Color32::from_black_alpha(220)))
                .show(ui, |ui| app.dev_ui(ui));
        });
    }
}

/// Three dots in the top-right corner: where to tap for the dev panel.
fn draw_dev_hotspot(shapes: &mut Shapes, frame: &Frame, open: bool) {
    shapes.view = View::PIXELS;
    let s = frame.scale;
    let center = Vec2::new(frame.size.x - DEV_HOTSPOT * 0.5 * s, DEV_HOTSPOT * 0.5 * s);
    let alpha = if open { 0.8 } else { 0.25 };
    for i in -1..=1 {
        shapes.circle(center + Vec2::new(i as f32 * 6.0 * s, 0.0), 1.8 * s, rgba(0xffffff, alpha));
    }
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
        let ctx = egui::Context::default();
        // The panel uses our game font instead of egui's bundled ones, which
        // would add about 2 MB to every web build.
        let mut fonts = egui::FontDefinitions::empty();
        fonts.font_data.insert("game".into(), Arc::new(egui::FontData::from_static(crate::text::FONT_BYTES)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.insert(family, vec!["game".into()]);
        }
        ctx.set_fonts(fonts);
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            &*graphics.window,
            Some(graphics.window.scale_factor() as f32),
            None,
            Some(graphics.device.limits().max_texture_dimension_2d as usize),
        );
        let renderer = egui_wgpu::Renderer::new(&graphics.device, graphics.config.format, Default::default());
        self.dev = Some(Dev { ctx, state, renderer, open: false });

        graphics.window.request_redraw();
        self.graphics = Some(graphics);
        self.last = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let panel_took_it = {
            let (Some(gfx), Some(dev)) = (self.graphics.as_ref(), self.dev.as_mut()) else { return };
            dev.open && dev.state.on_window_event(&gfx.window, &event).consumed
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                let gfx = self.graphics.as_mut().expect("graphics");
                gfx.resize(size.width, size.height);
                gfx.window.request_redraw();
            }
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && matches!(event.physical_key, PhysicalKey::Code(KeyCode::F1 | KeyCode::Backquote)) =>
            {
                if let Some(dev) = self.dev.as_mut() {
                    dev.open = !dev.open;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = Vec2::new(position.x as f32, position.y as f32);
                let pos = self.mouse;
                self.pointer(Pointer { id: 0, phase: Phase::Move, pos }, panel_took_it);
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let down = state == ElementState::Pressed;
                if down != self.mouse_down {
                    self.mouse_down = down;
                    let phase = if down { Phase::Down } else { Phase::Up };
                    let pos = self.mouse;
                    self.pointer(Pointer { id: 0, phase, pos }, panel_took_it);
                }
            }
            WindowEvent::Touch(t) => {
                let phase = match t.phase {
                    TouchPhase::Started => Phase::Down,
                    TouchPhase::Moved => Phase::Move,
                    TouchPhase::Ended => Phase::Up,
                    TouchPhase::Cancelled => Phase::Cancel,
                };
                let pos = Vec2::new(t.location.x as f32, t.location.y as f32);
                // Mouse is id 0, so touches start at 1.
                self.pointer(Pointer { id: t.id + 1, phase, pos }, panel_took_it);
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
                if let Some(gfx) = self.graphics.as_ref() {
                    gfx.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn draw(gfx: &mut Graphics, dev: &mut Dev, shapes: &Shapes, size: Vec2, panel: impl FnMut(&mut egui::Ui)) {
    use wgpu::CurrentSurfaceTexture as Current;
    // Always drain egui's input, so events don't pile up while the panel is shut.
    let raw = dev.state.take_egui_input(&gfx.window);

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

    if dev.open {
        let mut out = dev.ctx.run_ui(raw, panel);
        let mut textures = std::mem::take(&mut out.textures_delta);
        #[allow(unused_mut)]
        let mut platform = out.platform_output;
        #[cfg(target_arch = "wasm32")]
        platform.commands.retain(|command| match command {
            egui::OutputCommand::CopyText(text) => {
                copy_to_clipboard(text);
                false
            }
            _ => true,
        });
        dev.state.handle_platform_output(&gfx.window, platform);

        for (id, deltas) in &textures.set {
            for delta in deltas {
                dev.renderer.update_texture(&gfx.device, &gfx.queue, *id, delta);
            }
        }
        let jobs = dev.ctx.tessellate(out.shapes, out.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [gfx.config.width, gfx.config.height],
            pixels_per_point: out.pixels_per_point,
        };
        dev.renderer.update_buffers(&gfx.device, &gfx.queue, &mut encoder, &jobs, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("dev panel"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            dev.renderer.render(&mut pass, &jobs, &screen);
        }
        for id in &textures.free {
            dev.renderer.free_texture(id);
        }
        // Debug builds of epaint panic if deltas are dropped without this.
        textures.clear();
    }

    gfx.queue.submit([encoder.finish()]);
    gfx.window.pre_present_notify();
    gfx.queue.present(output);
}

#[cfg(target_arch = "wasm32")]
fn copy_to_clipboard(text: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.navigator().clipboard().write_text(text);
    }
}
