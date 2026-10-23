struct Globals {
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

struct Circle {
    @location(0) center: vec2<f32>,
    @location(1) radius: f32,
    @location(2) softness: f32,
    @location(3) color: vec4<f32>,
};

struct VOut {
    @builtin(position) pos: vec4<f32>,
    // Position relative to the circle's centre, in pixels.
    @location(0) local: vec2<f32>,
    @location(1) radius: f32,
    @location(2) softness: f32,
    @location(3) color: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32, c: Circle) -> VOut {
    var corners = array<vec2<f32>, 4>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0)
    );
    // Pad the quad so the falloff and the anti-aliasing pixel aren't clipped.
    let extent = c.radius + c.softness + 1.0;
    let local = corners[vi] * extent;
    let px = c.center + local;
    let ndc = vec2(px.x / g.viewport.x * 2.0 - 1.0, 1.0 - px.y / g.viewport.y * 2.0);

    var out: VOut;
    out.pos = vec4(ndc, 0.0, 1.0);
    out.local = local;
    out.radius = c.radius;
    out.softness = c.softness;
    out.color = c.color;
    return out;
}

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> {
    let d = length(v.local) - v.radius;
    // A one-pixel ramp gives a crisp anti-aliased edge; softness widens it into a glow.
    let ramp = max(v.softness, 1.0);
    let coverage = clamp(0.5 - d / ramp, 0.0, 1.0);
    let a = v.color.a * coverage;
    return vec4(v.color.rgb * a, a);
}
