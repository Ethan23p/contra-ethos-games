//! Offscreen rendering to PNG, native only. Lets us check visuals without a
//! browser: render an [`App`] at chosen times and look at the images.

use std::path::Path;

use glam::Vec2;

use crate::gpu::request_device;
use crate::runner::{App, Frame};
use crate::shapes::{ShapeRenderer, Shapes, View};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Renders `app` at each time in `times` (seconds) and writes the frames side by
/// side, left to right, into one PNG strip at `path`.
pub fn snapshot_strip<A: App>(app: &mut A, width: u32, height: u32, times: &[f32], path: &Path) {
    let instance = wgpu::Instance::default();
    let (_adapter, device, queue) = pollster::block_on(request_device(&instance, None));
    let mut renderer = ShapeRenderer::new(&device, &queue, FORMAT);

    let mut strip = image::RgbaImage::new(width * times.len() as u32, height);
    let mut shapes = Shapes::default();
    // Same fixed steps as the live loop, so a snapshot shows exactly what a
    // player would see at that moment.
    let step = app.step();
    let (mut last, mut simulated) = (0.0, 0.0);
    for (i, &time) in times.iter().enumerate() {
        while simulated + step <= time {
            app.update(step);
            simulated += step;
        }
        let size = Vec2::new(width as f32, height as f32);
        let frame = Frame { time, dt: time - last, size, scale: 1.0, alpha: (time - simulated) / step };
        last = time;
        shapes.clear_shapes();
        shapes.view = View::PIXELS;
        app.frame(&frame, &mut shapes);
        let pixels = render_once(&device, &queue, &mut renderer, &shapes, width, height);
        let tile = image::RgbaImage::from_raw(width, height, pixels).expect("frame size");
        image::imageops::replace(&mut strip, &tile, (i as u32 * width) as i64, 0);
    }
    strip.save(path).expect("write png");
}

fn render_once(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut ShapeRenderer,
    shapes: &Shapes,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("snapshot"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());

    // Rows in a texture-to-buffer copy must be padded to 256 bytes.
    let row = width * 4;
    let padded_row = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("snapshot readback"),
        size: (padded_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&Default::default());
    renderer.render(device, queue, &mut encoder, &view, Vec2::new(width as f32, height as f32), shapes);
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |result| result.expect("map readback"));
    device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");

    let data = slice.get_mapped_range().expect("mapped range");
    let mut pixels = Vec::with_capacity((row * height) as usize);
    for y in 0..height as usize {
        let start = y * padded_row as usize;
        pixels.extend_from_slice(&data[start..start + row as usize]);
    }
    pixels
}
