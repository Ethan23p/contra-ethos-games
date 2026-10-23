//! Instanced SDF shapes. One quad per shape; the fragment shader evaluates the
//! shape's distance field, which gives exact anti-aliasing at any size.
//!
//! Coordinates are physical pixels with the origin at the top left. Colours are
//! authored in sRGB and blended in sRGB space (the surface format is non-sRGB on
//! every platform), so what we pick is what appears, on desktop and in the browser.

use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use wgpu::util::DeviceExt;

/// One circle instance, laid out exactly as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Circle {
    pub center: [f32; 2],
    pub radius: f32,
    /// Width of the edge falloff in pixels: 0 gives a crisp anti-aliased edge,
    /// larger values give a soft glow.
    pub softness: f32,
    pub color: [f32; 4],
}

/// What an [`crate::App`] fills in each frame.
pub struct Shapes {
    pub clear: [f32; 4],
    pub(crate) circles: Vec<Circle>,
}

impl Default for Shapes {
    fn default() -> Self {
        Self { clear: rgb(0x111111), circles: Vec::new() }
    }
}

impl Shapes {
    pub fn circle(&mut self, center: Vec2, radius: f32, color: [f32; 4]) {
        self.soft_circle(center, radius, 0.0, color);
    }

    pub fn soft_circle(&mut self, center: Vec2, radius: f32, softness: f32, color: [f32; 4]) {
        self.circles.push(Circle { center: center.into(), radius, softness, color });
    }

    pub(crate) fn clear_shapes(&mut self) {
        self.circles.clear();
    }
}

/// An opaque sRGB colour from a hex literal, e.g. `rgb(0xe8c547)`.
pub fn rgb(hex: u32) -> [f32; 4] {
    rgba(hex, 1.0)
}

pub fn rgba(hex: u32, alpha: f32) -> [f32; 4] {
    let channel = |shift: u32| ((hex >> shift) & 0xff) as f32 / 255.0;
    [channel(16), channel(8), channel(0), alpha]
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    viewport: [f32; 2],
    _pad: [f32; 2],
}

pub(crate) struct ShapeRenderer {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instances: wgpu::Buffer,
    capacity: usize,
}

impl ShapeRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shapes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shapes.wgsl").into()),
        });

        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shape globals"),
            contents: bytemuck::bytes_of(&Globals { viewport: [1.0, 1.0], _pad: [0.0; 2] }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shape globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shape globals"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shapes"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<Circle>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x2, 1 => Float32, 2 => Float32, 3 => Float32x4
            ],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("shapes"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(instance_layout)],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let capacity = 256;
        Self { pipeline, globals, bind_group, instances: instance_buffer(device, capacity), capacity }
    }

    /// Records one pass that clears `target` and draws everything in `shapes`.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: Vec2,
        shapes: &Shapes,
    ) {
        let globals = Globals { viewport: size.into(), _pad: [0.0; 2] };
        queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let count = shapes.circles.len();
        if count > self.capacity {
            self.capacity = count.next_power_of_two();
            self.instances = instance_buffer(device, self.capacity);
        }
        if count > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&shapes.circles));
        }

        let [r, g, b, a] = shapes.clear.map(f64::from);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shapes"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        if count > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            pass.draw(0..4, 0..count as u32);
        }
    }
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shape instances"),
        size: (capacity * size_of::<Circle>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
