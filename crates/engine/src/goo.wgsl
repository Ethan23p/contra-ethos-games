// Goo: a group of circles and links drawn as one smooth union. Every circle
// draws its own quad, but a pixel is kept only by the circle nearest it, so
// the union shows once, seamlessly, however its quads overlap.

struct Globals {
    viewport: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
// Circles as (centre, radius, 0) then links as (a, b), all in pixels.
@group(0) @binding(1) var<storage, read> members: array<vec4<f32>>;

struct Instance {
    // quad centre, quad half-size, owner index, unused
    @location(0) quad: vec4<f32>,
    // first member, circles, links, unused
    @location(1) range: vec4<u32>,
    // blend, link radius (pixels), sheen position, sheen strength
    @location(2) shape: vec4<f32>,
    @location(3) fill: vec4<f32>,
    @location(4) sheen_color: vec4<f32>,
};

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) px: vec2<f32>,
    @location(1) @interpolate(flat) owner: u32,
    @location(2) @interpolate(flat) range: vec4<u32>,
    @location(3) @interpolate(flat) shape: vec4<f32>,
    @location(4) @interpolate(flat) fill: vec4<f32>,
    @location(5) @interpolate(flat) sheen_color: vec4<f32>,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32, i: Instance) -> VOut {
    var corners = array<vec2<f32>, 4>(
        vec2(-1.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 1.0), vec2(1.0, 1.0)
    );
    let px = i.quad.xy + corners[vi] * i.quad.z;
    var out: VOut;
    out.pos = vec4(px.x / g.viewport.x * 2.0 - 1.0, 1.0 - px.y / g.viewport.y * 2.0, 0.0, 1.0);
    out.px = px;
    out.owner = u32(i.quad.w);
    out.range = i.range;
    out.shape = i.shape;
    out.fill = i.fill;
    out.sheen_color = i.sheen_color;
    return out;
}

// Polynomial smooth minimum of two distances: blends within k of the seam,
// exact beyond it.
fn smin(a: f32, b: f32, k: f32) -> f32 {
    let h = max(k - abs(a - b), 0.0) / max(k, 1e-6);
    return min(a, b) - h * h * k * 0.25;
}

fn capsule(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let ab = b - a;
    let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return length(p - (a + ab * t)) - r;
}

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> {
    let p = v.px;
    let start = v.range.x;
    let circles = v.range.y;
    let k = v.shape.x;

    var d = 1e9;
    var nearest = 1e9;
    var owner = 0u;
    for (var j = 0u; j < circles; j++) {
        let c = members[start + j];
        let dj = length(p - c.xy) - c.z;
        d = smin(d, dj, k);
        if dj < nearest {
            nearest = dj;
            owner = j;
        }
    }
    for (var j = 0u; j < v.range.z; j++) {
        let l = members[start + circles + j];
        d = smin(d, capsule(p, l.xy, l.zw, v.shape.y), k);
    }
    if owner != v.owner {
        discard;
    }
    let coverage = clamp(0.5 - d, 0.0, 1.0);
    if coverage <= 0.0 {
        discard;
    }

    // Sheen: a band sweeping across the group, measured from its first circle.
    let first = members[start];
    let u = dot(p - first.xy, normalize(vec2(1.0, 0.55))) / first.z;
    let band = exp(-pow((u - v.shape.z) / 0.32, 2.0)) * v.shape.w;
    let rgb = mix(v.fill.rgb, v.sheen_color.rgb, clamp(band, 0.0, 1.0));
    return vec4(rgb * v.fill.a, v.fill.a) * coverage;
}
