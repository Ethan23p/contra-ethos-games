//! Goo: circles and the links between them drawn as one smooth union, so
//! neighbours fuse at a touch and merging blobs flow together. Lengths are
//! world units through the current [`crate::View`], like every other shape.
//!
//! Each circle draws its own quad and keeps only the pixels it's nearest, so
//! a group renders once without seams, and a whole field costs one draw call.

use bytemuck::{Pod, Zeroable};
use glam::Vec2;

use crate::shapes::{Layer, Shapes};

/// How a goo group looks. Lengths are world units.
#[derive(Clone, Copy, Debug)]
pub struct Goo {
    pub fill: [f32; 4],
    pub outline: [f32; 4],
    pub outline_width: f32,
    /// How far apart two surfaces start to fuse.
    pub blend: f32,
    /// Radius of the tether a link draws.
    pub link_radius: f32,
    /// Strength of the specular spot on each circle, 0–1.
    pub highlight: f32,
    /// How strongly the edges shade toward and away from the light.
    pub shading: f32,
    /// Where the sheen band sits across the group, in radii of its first
    /// circle from that circle's centre, along a diagonal.
    pub sheen_at: f32,
    /// The sheen band's opacity; 0 turns it off.
    pub sheen: f32,
    pub sheen_color: [f32; 4],
}

impl Default for Goo {
    fn default() -> Self {
        Self {
            fill: [1.0; 4],
            outline: [0.0, 0.0, 0.0, 0.25],
            outline_width: 0.0,
            blend: 0.0,
            link_radius: 0.0,
            highlight: 0.0,
            shading: 0.0,
            sheen_at: 0.0,
            sheen: 0.0,
            sheen_color: [1.0; 4],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GooInstance {
    quad: [f32; 4],
    range: [u32; 4],
    shape: [f32; 4],
    fill: [f32; 4],
    outline: [f32; 4],
    light: [f32; 4],
    sheen_color: [f32; 4],
}

impl Shapes {
    /// One goo group: `circles` as (centre, radius), fused wherever they come
    /// within `style.blend`, and `links` as pairs of circle indices drawn as
    /// tethers between them.
    pub fn goo(&mut self, circles: &[(Vec2, f32)], links: &[(usize, usize)], style: &Goo) {
        if circles.is_empty() {
            return;
        }
        let v = self.view;
        let start = self.goo_members.len() as u32;
        let px: Vec<(Vec2, f32)> = circles.iter().map(|&(c, r)| (v.to_screen(c), r * v.scale)).collect();
        for &(c, r) in &px {
            self.goo_members.push([c.x, c.y, r, 0.0]);
        }
        let links: Vec<(usize, usize)> =
            links.iter().copied().filter(|&(a, b)| a < px.len() && b < px.len() && a != b).collect();
        for &(a, b) in &links {
            self.goo_members.push([px[a].0.x, px[a].0.y, px[b].0.x, px[b].0.y]);
        }

        let (blend, link) = (style.blend * v.scale, style.link_radius * v.scale);
        let first = self.goo_instances.len();
        for (i, &(c, r)) in px.iter().enumerate() {
            // Big enough for everything this circle can be nearest to: its own
            // blended edge, and half of each of its tethers.
            let mut half = r + blend;
            for &(a, b) in &links {
                if a == i || b == i {
                    half = half.max(px[a].0.distance(px[b].0) * 0.5 + link + blend);
                }
            }
            self.goo_instances.push(GooInstance {
                quad: [c.x, c.y, half + 2.0, i as f32],
                range: [start, px.len() as u32, links.len() as u32, 0],
                shape: [blend, style.outline_width * v.scale, link, 0.0],
                fill: style.fill,
                outline: style.outline,
                light: [style.highlight, style.shading, style.sheen_at, style.sheen],
                sheen_color: style.sheen_color,
            });
        }
        self.mark(Layer::Goo, first, self.goo_instances.len());
    }
}

pub(crate) struct GooPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    pub instances: wgpu::Buffer,
    capacity: usize,
    members: wgpu::Buffer,
    member_capacity: usize,
}

impl GooPipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, globals: &wgpu::Buffer) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("goo"),
            source: wgpu::ShaderSource::Wgsl(include_str!("goo.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("goo"),
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
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("goo"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: size_of::<GooInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x4, 1 => Uint32x4, 2 => Float32x4, 3 => Float32x4,
                4 => Float32x4, 5 => Float32x4, 6 => Float32x4
            ],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("goo"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(instance_layout)],
            },
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
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
        let (capacity, member_capacity) = (64, 256);
        let members = member_buffer(device, member_capacity);
        let bind_group = bind(device, &layout, globals, &members);
        Self {
            pipeline,
            layout,
            bind_group,
            instances: instance_buffer(device, capacity),
            capacity,
            members,
            member_capacity,
        }
    }

    /// Uploads this frame's goo, growing the buffers as needed.
    pub fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, globals: &wgpu::Buffer, shapes: &Shapes) {
        let (count, members) = (shapes.goo_instances.len(), shapes.goo_members.len());
        if count > self.capacity {
            self.capacity = count.next_power_of_two();
            self.instances = instance_buffer(device, self.capacity);
        }
        if members > self.member_capacity {
            self.member_capacity = members.next_power_of_two();
            self.members = member_buffer(device, self.member_capacity);
            self.bind_group = bind(device, &self.layout, globals, &self.members);
        }
        if count > 0 {
            queue.write_buffer(&self.instances, 0, bytemuck::cast_slice(&shapes.goo_instances));
        }
        if members > 0 {
            queue.write_buffer(&self.members, 0, bytemuck::cast_slice(&shapes.goo_members));
        }
    }

    pub fn bind(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
    }
}

fn bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    members: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("goo"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: members.as_entire_binding() },
        ],
    })
}

fn instance_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("goo instances"),
        size: (capacity * size_of::<GooInstance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn member_buffer(device: &wgpu::Device, capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("goo members"),
        size: (capacity * 16) as u64,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_draws_one_quad_per_circle_covering_its_links() {
        let mut s = Shapes::default();
        let style = Goo { blend: 4.0, link_radius: 3.0, ..Goo::default() };
        s.goo(&[(Vec2::ZERO, 10.0), (Vec2::new(60.0, 0.0), 10.0)], &[(0, 1)], &style);
        assert_eq!(s.goo_instances.len(), 2);
        assert_eq!(s.goo_members.len(), 3);
        // Half the tether (30) + its radius + the blend, plus the padding.
        assert_eq!(s.goo_instances[0].quad[2], 30.0 + 3.0 + 4.0 + 2.0);
        assert_eq!(s.goo_instances[1].quad[3], 1.0);
    }

    #[test]
    fn layers_keep_draw_order() {
        let mut s = Shapes::default();
        s.circle(Vec2::ZERO, 5.0, [1.0; 4]);
        s.goo(&[(Vec2::ZERO, 5.0)], &[], &Goo::default());
        s.goo(&[(Vec2::ZERO, 5.0)], &[], &Goo::default());
        s.circle(Vec2::ZERO, 5.0, [1.0; 4]);
        let layers: Vec<Layer> = s.batches.iter().map(|b| b.0).collect();
        assert_eq!(layers, vec![Layer::Shapes, Layer::Goo, Layer::Shapes]);
        assert_eq!(s.batches[1].1, 0..2);
    }
}
