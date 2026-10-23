struct Globals {
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct Instance {
    @location(0) center: vec2<f32>,
    @location(1) half: vec2<f32>,
    // corner radius, rotation, softness, outline width
    @location(2) params: vec4<f32>,
    @location(3) fill: vec4<f32>,
    @location(4) outline: vec4<f32>,
    @location(5) uv: vec4<f32>,
    @location(6) sdf_scale: f32,
};

struct VOut {
    @builtin(position) pos: vec4<f32>,
    // Position in the shape's own frame (unrotated), pixels from its centre.
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) half: vec2<f32>,
    @location(2) @interpolate(flat) params: vec4<f32>,
    @location(3) @interpolate(flat) fill: vec4<f32>,
    @location(4) @interpolate(flat) outline: vec4<f32>,
    @location(5) @interpolate(flat) uv: vec4<f32>,
    @location(6) @interpolate(flat) sdf_scale: f32,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32, i: Instance) -> VOut {
    var corners = array<vec2<f32>, 4>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0)
    );
    // Pad the quad so the falloff and the anti-aliasing pixel aren't clipped.
    let local = corners[vi] * (i.half + vec2(i.params.z + 1.0));
    let c = cos(i.params.y);
    let s = sin(i.params.y);
    let px = i.center + vec2(local.x * c - local.y * s, local.x * s + local.y * c);
    let ndc = vec2(px.x / g.viewport.x * 2.0 - 1.0, 1.0 - px.y / g.viewport.y * 2.0);

    var out: VOut;
    out.pos = vec4(ndc, 0.0, 1.0);
    out.local = local;
    out.half = i.half;
    out.params = i.params;
    out.fill = i.fill;
    out.outline = i.outline;
    out.uv = i.uv;
    out.sdf_scale = i.sdf_scale;
    return out;
}

fn rounded_box(p: vec2<f32>, half: vec2<f32>, corner: f32) -> f32 {
    let r = min(corner, min(half.x, half.y));
    let q = abs(p) - half + vec2(r);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> {
    var d: f32;
    if v.sdf_scale > 0.0 {
        // Glyph: read the distance field, encoded with the edge at 0.5.
        let t = clamp(v.local / v.half * 0.5 + 0.5, vec2(0.0), vec2(1.0));
        let field = textureSampleLevel(atlas, atlas_sampler, mix(v.uv.xy, v.uv.zw, t), 0.0).r;
        d = (0.5 - field) * v.sdf_scale;
    } else {
        d = rounded_box(v.local, v.half, v.params.x);
    }

    // Shapes get an inner outline, so it never changes their footprint; glyphs
    // an outer one, so thin strokes don't hollow out.
    let w = v.params.w;
    var edge = d;
    var inner = d + w;
    if v.sdf_scale > 0.0 {
        edge = d - w;
        inner = d;
    }

    // A one-pixel ramp gives a crisp anti-aliased edge; softness widens it into a glow.
    let coverage = clamp(0.5 - edge / max(v.params.z, 1.0), 0.0, 1.0);
    let fill = vec4(v.fill.rgb * v.fill.a, v.fill.a);
    var color = fill;
    if w > 0.0 {
        let outline = vec4(v.outline.rgb * v.outline.a, v.outline.a);
        color = mix(outline, fill, clamp(0.5 - inner, 0.0, 1.0));
    }
    return color * coverage;
}
