//! Instanced SDF shapes and text. One quad per shape or glyph; the fragment
//! shader evaluates its distance field, which gives exact anti-aliasing at any
//! size and makes outlines and glow nearly free.
//!
//! Apps draw in world coordinates through a [`View`]; the renderer works in
//! physical pixels with the origin at the top left. Colours are authored in sRGB
//! and blended in sRGB space (the surface format is non-sRGB on every platform),
//! so what we pick is what appears, on desktop and in the browser.

use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use wgpu::util::DeviceExt;

use crate::text::{FONT, Font, SPREAD};

/// One shape or glyph, laid out exactly as the shader reads it.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Instance {
    center: [f32; 2],
    half: [f32; 2],
    corner: f32,
    rotation: f32,
    /// Width of the edge falloff in pixels: 0 gives a crisp anti-aliased edge,
    /// larger values a soft glow.
    softness: f32,
    outline_width: f32,
    fill: [f32; 4],
    outline: [f32; 4],
    /// Glyph rectangle in the atlas; unused for shapes.
    uv: [f32; 4],
    /// Atlas field value to pixels, for glyphs. Zero marks a plain shape.
    sdf_scale: f32,
    _pad: [f32; 3],
}

/// Maps world coordinates to physical pixels: `screen = offset + world * scale`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub scale: f32,
    pub offset: Vec2,
}

impl View {
    pub const PIXELS: Self = Self { scale: 1.0, offset: Vec2::ZERO };

    /// Fits a world `size` wide and high, centred on the world origin, inside
    /// `viewport` (physical pixels), keeping its aspect ratio.
    pub fn fit(size: Vec2, viewport: Vec2) -> Self {
        let scale = (viewport / size).min_element();
        Self { scale, offset: viewport * 0.5 }
    }

    pub fn to_screen(&self, world: Vec2) -> Vec2 {
        self.offset + world * self.scale
    }

    pub fn to_world(&self, screen: Vec2) -> Vec2 {
        (screen - self.offset) / self.scale
    }
}

impl Default for View {
    fn default() -> Self {
        Self::PIXELS
    }
}

/// What an [`crate::App`] fills in each frame. Later calls draw on top.
pub struct Shapes {
    pub clear: [f32; 4],
    /// Applied to everything pushed after it is set.
    pub view: View,
    pub(crate) instances: Vec<Instance>,
}

impl Default for Shapes {
    fn default() -> Self {
        Self { clear: rgb(0x111111), view: View::PIXELS, instances: Vec::new() }
    }
}

impl Shapes {
    pub fn circle(&mut self, center: Vec2, radius: f32, color: [f32; 4]) -> Styled<'_> {
        self.rounded_rect(center, Vec2::splat(radius * 2.0), radius, color)
    }

    /// An axis-aligned rectangle of full `size`; round it with [`Styled::corner`].
    pub fn rect(&mut self, center: Vec2, size: Vec2, color: [f32; 4]) -> Styled<'_> {
        self.rounded_rect(center, size, 0.0, color)
    }

    pub fn rounded_rect(&mut self, center: Vec2, size: Vec2, corner: f32, color: [f32; 4]) -> Styled<'_> {
        let (v, start) = (self.view, self.instances.len());
        self.instances.push(Instance {
            center: v.to_screen(center).into(),
            half: (size * 0.5 * v.scale).into(),
            corner: corner * v.scale,
            fill: color,
            ..Zeroable::zeroed()
        });
        Styled { items: &mut self.instances[start..], pivot: v.to_screen(center), scale: v.scale }
    }

    /// One line of text at font size `size`, centred on `center` (horizontally on
    /// its advance width, vertically on the height of a digit).
    pub fn text(&mut self, center: Vec2, size: f32, text: &str, color: [f32; 4]) -> Styled<'_> {
        let font = &*FONT;
        let (v, start) = (self.view, self.instances.len());
        let px = size * v.scale;
        let k = Font::scale(px);
        let pivot = v.to_screen(center);
        let mut pen = pivot + Vec2::new(-font.width(text, px) * 0.5, font.cap_height * k * 0.5);
        for g in text.chars().filter_map(|c| font.glyph(c)) {
            let half = g.size * k * 0.5;
            self.instances.push(Instance {
                center: (pen + g.offset * k + half).into(),
                half: half.into(),
                fill: color,
                uv: g.uv,
                sdf_scale: 2.0 * SPREAD * k,
                ..Zeroable::zeroed()
            });
            pen.x += g.advance * k;
        }
        Styled { items: &mut self.instances[start..], pivot, scale: v.scale }
    }

    /// Width of `text` at font size `size`, in world units.
    pub fn text_width(&self, size: f32, text: &str) -> f32 {
        FONT.width(text, size)
    }

    pub(crate) fn clear_shapes(&mut self) {
        self.instances.clear();
    }
}

/// Styling for what was just drawn. Lengths are in world units, like the shape.
pub struct Styled<'a> {
    items: &'a mut [Instance],
    pivot: Vec2,
    scale: f32,
}

impl Styled<'_> {
    pub fn corner(self, radius: f32) -> Self {
        for i in self.items.iter_mut() {
            i.corner = radius * self.scale;
        }
        self
    }

    /// An outline `width` thick: inside the edge for shapes, outside it for
    /// text. Text outlines can reach about 8 px at 64 px type, scaling with size.
    pub fn outline(self, width: f32, color: [f32; 4]) -> Self {
        for i in self.items.iter_mut() {
            i.outline_width = width * self.scale;
            i.outline = color;
        }
        self
    }

    /// Widens the edge into a soft falloff `width` wide, centred on the edge.
    pub fn soft(self, width: f32) -> Self {
        for i in self.items.iter_mut() {
            i.softness = width * self.scale;
        }
        self
    }

    /// Rotates by `angle` radians about the shape's centre (for text, the
    /// centre of the whole line).
    pub fn rotate(self, angle: f32) -> Self {
        let r = Vec2::from_angle(angle);
        for i in self.items.iter_mut() {
            let c = Vec2::from(i.center);
            i.center = (self.pivot + r.rotate(c - self.pivot)).into();
            i.rotation += angle;
        }
        self
    }

    /// Scales about the same centre as [`Styled::rotate`].
    pub fn scale(self, factor: f32) -> Self {
        for i in self.items.iter_mut() {
            let c = Vec2::from(i.center);
            i.center = (self.pivot + (c - self.pivot) * factor).into();
            i.half = (Vec2::from(i.half) * factor).into();
            i.corner *= factor;
            i.outline_width *= factor;
            i.sdf_scale *= factor;
        }
        self
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
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shapes"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shapes.wgsl").into()),
        });

        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("shape globals"),
            contents: bytemuck::bytes_of(&Globals { viewport: [1.0, 1.0], _pad: [0.0; 2] }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let font = &*FONT;
        let (width, height) = font.atlas_size;
        let atlas = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("glyph atlas"),
                size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            Default::default(),
            &font.atlas,
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph atlas"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shapes"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shapes"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas.create_view(&Default::default())),
                },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("shapes"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<Instance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x2, 1 => Float32x2, 2 => Float32x4,
                3 => Float32x4, 4 => Float32x4, 5 => Float32x4, 6 => Float32
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

        let count = shapes.instances.len();
        if count > self.capacity {
            self.capacity = count.next_power_of_two();
            self.instances = instance_buffer(device, self.capacity);
        }
        if count > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&shapes.instances));
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
        size: (capacity * size_of::<Instance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_round_trips() {
        let v = View::fit(Vec2::new(10.0, 20.0), Vec2::new(400.0, 400.0));
        assert_eq!(v.scale, 20.0);
        assert_eq!(v.to_screen(Vec2::ZERO), Vec2::new(200.0, 200.0));
        let p = Vec2::new(3.0, -4.0);
        assert!((v.to_world(v.to_screen(p)) - p).length() < 1e-5);
    }

    #[test]
    fn styling_reaches_every_glyph() {
        let mut s = Shapes::default();
        s.text(Vec2::ZERO, 20.0, "128", rgb(0xffffff)).outline(2.0, rgb(0));
        assert_eq!(s.instances.len(), 3);
        assert!(s.instances.iter().all(|i| i.outline_width == 2.0 && i.sdf_scale > 0.0));
    }
}
