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
use crate::panel::Panel;
use crate::shapes::{Shapes, View};

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

impl Frame {
    /// The viewport's anchors and units, for placing UI.
    pub fn screen(&self) -> crate::Screen {
        crate::Screen::new(self.size, self.scale)
    }
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

    /// The game's tuner, if it has one: the config panel's button shows only then.
    fn config(&mut self) -> Option<&mut dyn crate::tune::Config> {
        None
    }

    /// Extra controls at the top of the panel's option island ("New game", say).
    fn config_actions(&mut self, _ui: &mut egui::Ui) {}

    /// Whether the pinned-cards widget sits at the top of the screen, on the
    /// button's side, instead of just above the button.
    fn config_widget_top(&self) -> bool {
        false
    }
}

/// The longest stretch one frame may simulate, so a stall (a breakpoint, a tab
/// in the background) doesn't turn into a burst of thousands of steps.
const MAX_CATCH_UP: f32 = 0.25;

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
    panel: Panel,
}

struct Runner<A: App> {
    app: A,
    proxy: Option<EventLoopProxy<Graphics>>,
    graphics: Option<Graphics>,
    dev: Option<Dev>,
    shapes: Shapes,
    gestures: Gestures,
    inputs: Vec<Input>,
    /// Pointers that went down on the game, not the overlay; only these reach it.
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

    fn pointer(&mut self, p: Pointer) {
        let frame = self.frame_info(0.0);
        // The overlay decides on the press itself, from where it was drawn last
        // frame: an open panel takes every press, a shut one only its button
        // and pinned widget. egui sees every event either way.
        let overlay = self.dev.as_ref().is_some_and(|d| {
            let at = p.pos / d.ctx.pixels_per_point();
            d.panel.takes(egui::pos2(at.x, at.y))
        });
        if p.phase == Phase::Down {
            if overlay {
                return;
            }
            self.owned.push(p.id);
        } else if !self.owned.contains(&p.id) {
            // Hover moves from the mouse still reach the game, unless over the overlay.
            if !(p.id == 0 && p.phase == Phase::Move && !overlay) {
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

        let gfx = self.graphics.as_mut().expect("graphics");
        let dev = self.dev.as_mut().expect("overlay");
        let (app, panel) = (&mut self.app, &mut dev.panel);
        draw(gfx, &dev.ctx, &mut dev.state, &mut dev.renderer, &self.shapes, frame.size, |ui| {
            panel.show(ui.ctx(), app);
        });
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
        Panel::style(&ctx);
        let state = egui_winit::State::new(
            ctx.clone(),
            egui::ViewportId::ROOT,
            &*graphics.window,
            Some(graphics.window.scale_factor() as f32),
            None,
            Some(graphics.device.limits().max_texture_dimension_2d as usize),
        );
        let renderer = egui_wgpu::Renderer::new(&graphics.device, graphics.config.format, Default::default());
        self.dev = Some(Dev { ctx, state, renderer, panel: Panel::new() });

        graphics.window.request_redraw();
        self.graphics = Some(graphics);
        self.last = Instant::now();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        {
            let (Some(gfx), Some(dev)) = (self.graphics.as_ref(), self.dev.as_mut()) else { return };
            // Always, so egui never misses half of a press and keeps a stuck pointer.
            let _ = dev.state.on_window_event(&gfx.window, &event);
        }

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
                    dev.panel.open = !dev.panel.open;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.mouse = Vec2::new(position.x as f32, position.y as f32);
                let pos = self.mouse;
                self.pointer(Pointer { id: 0, phase: Phase::Move, pos });
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let down = state == ElementState::Pressed;
                if down != self.mouse_down {
                    self.mouse_down = down;
                    let phase = if down { Phase::Down } else { Phase::Up };
                    let pos = self.mouse;
                    self.pointer(Pointer { id: 0, phase, pos });
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
                self.pointer(Pointer { id: t.id + 1, phase, pos });
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

fn draw(
    gfx: &mut Graphics,
    ctx: &egui::Context,
    state: &mut egui_winit::State,
    renderer: &mut egui_wgpu::Renderer,
    shapes: &Shapes,
    size: Vec2,
    overlay: impl FnMut(&mut egui::Ui),
) {
    use wgpu::CurrentSurfaceTexture as Current;
    // egui runs every frame, even one the surface skips, so no press, release
    // or texture update is ever dropped.
    let raw = state.take_egui_input(&gfx.window);
    let mut out = ctx.run_ui(raw, overlay);
    let textures = std::mem::take(&mut out.textures_delta);
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
    state.handle_platform_output(&gfx.window, platform);
    for (id, deltas) in &textures.set {
        for delta in deltas {
            renderer.update_texture(&gfx.device, &gfx.queue, *id, delta);
        }
    }
    let free = |renderer: &mut egui_wgpu::Renderer, mut textures: egui::TexturesDelta| {
        for id in &textures.free {
            renderer.free_texture(id);
        }
        // Debug builds of epaint panic if deltas are dropped without this.
        textures.clear();
    };

    let output = match gfx.surface.get_current_texture() {
        Current::Success(output) => output,
        Current::Suboptimal(output) => {
            gfx.surface.configure(&gfx.device, &gfx.config);
            output
        }
        Current::Outdated | Current::Lost => {
            // After a resize or tab switch: reconfigure and draw next frame.
            gfx.surface.configure(&gfx.device, &gfx.config);
            free(renderer, textures);
            return;
        }
        other => {
            log::warn!("skipping frame: {other:?}");
            free(renderer, textures);
            return;
        }
    };
    let view = output.texture.create_view(&Default::default());
    let mut encoder = gfx.device.create_command_encoder(&Default::default());
    gfx.shapes.render(&gfx.device, &gfx.queue, &mut encoder, &view, size, shapes);

    let jobs = ctx.tessellate(out.shapes, out.pixels_per_point);
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [gfx.config.width, gfx.config.height],
        pixels_per_point: out.pixels_per_point,
    };
    renderer.update_buffers(&gfx.device, &gfx.queue, &mut encoder, &jobs, &screen);
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        renderer.render(&mut pass, &jobs, &screen);
    }
    free(renderer, textures);

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
