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
    // blend, outline width, link radius, unused (pixels)
    @location(2) shape: vec4<f32>,
    @location(3) fill: vec4<f32>,
    @location(4) outline: vec4<f32>,
    // highlight, shading, sheen position, sheen strength
    @location(5) light: vec4<f32>,
    @location(6) sheen_color: vec4<f32>,
};

struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) px: vec2<f32>,
    @location(1) @interpolate(flat) owner: u32,
    @location(2) @interpolate(flat) range: vec4<u32>,
    @location(3) @interpolate(flat) shape: vec4<f32>,
    @location(4) @interpolate(flat) fill: vec4<f32>,
    @location(5) @interpolate(flat) outline: vec4<f32>,
    @location(6) @interpolate(flat) light: vec4<f32>,
    @location(7) @interpolate(flat) sheen_color: vec4<f32>,
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
    out.outline = i.outline;
    out.light = i.light;
    out.sheen_color = i.sheen_color;
    return out;
}

// Polynomial smooth minimum of two distances, each carried with its gradient
// as (d, ∂x, ∂y): blends within k of the seam, exact beyond it. The gradient
// comes out analytically, so shading needs no screen-space derivatives.
fn smin(a: vec3<f32>, b: vec3<f32>, k: f32) -> vec3<f32> {
    let lo = select(b, a, a.x < b.x);
    let hi = select(a, b, a.x < b.x);
    let h = max(k - (hi.x - lo.x), 0.0) / max(k, 1e-6);
    return vec3(lo.x - h * h * k * 0.25, mix(lo.yz, hi.yz, h * 0.5));
}

fn circle(p: vec2<f32>, c: vec2<f32>, r: f32) -> vec3<f32> {
    let q = p - c;
    let l = length(q);
    return vec3(l - r, q / max(l, 1e-4));
}

fn capsule(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, r: f32) -> vec3<f32> {
    let ab = b - a;
    let t = clamp(dot(p - a, ab) / max(dot(ab, ab), 1e-6), 0.0, 1.0);
    return circle(p, a + ab * t, r);
}

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> {
    let p = v.px;
    let start = v.range.x;
    let circles = v.range.y;
    let k = v.shape.x;

    var f = vec3(1e9, 0.0, 0.0);
    var nearest = 1e9;
    var owner = 0u;
    var spot = 0.0;
    for (var j = 0u; j < circles; j++) {
        let c = members[start + j];
        let fj = circle(p, c.xy, c.z);
        f = smin(f, fj, k);
        if fj.x < nearest {
            nearest = fj.x;
            owner = j;
        }
        // A highlight on every circle, taken across the group so it never
        // cuts off where ownership changes.
        let s = (p - (c.xy + vec2(-0.32, -0.38) * c.z)) / (0.3 * c.z);
        spot = max(spot, exp(-dot(s, s)));
    }
    for (var j = 0u; j < v.range.z; j++) {
        let l = members[start + circles + j];
        f = smin(f, capsule(p, l.xy, l.zw, v.shape.z), k);
    }
    let d = f.x;
    if owner != v.owner {
        discard;
    }

    let coverage = clamp(0.5 - d, 0.0, 1.0);
    if coverage <= 0.0 {
        discard;
    }
    let own = members[start + owner];
    let depth = clamp(-d / own.z, 0.0, 1.0);

    // A soft dome lit from the top left: the edge facing the light brightens,
    // the far edge darkens, fading toward the middle.
    let n = f.yz / max(length(f.yz), 1e-4);
    let facing = dot(n, normalize(vec2(-0.6, -0.8)));
    let edge = (1.0 - depth) * (1.0 - depth);
    var rgb = v.fill.rgb * (1.0 + v.light.y * facing * edge);

    // Sheen: a band sweeping across the group, measured from its first circle.
    let first = members[start];
    let u = dot(p - first.xy, normalize(vec2(1.0, 0.55))) / first.z;
    let band = exp(-pow((u - v.light.z) / 0.32, 2.0)) * v.light.w;
    rgb = mix(rgb, v.sheen_color.rgb, clamp(band, 0.0, 1.0));

    rgb = mix(rgb, vec3(1.0), v.light.x * spot);

    // An inner outline, so it never changes the footprint.
    let w = v.shape.y;
    var color = vec4(rgb * v.fill.a, v.fill.a);
    if w > 0.0 {
        let o = vec4(v.outline.rgb * v.outline.a, v.outline.a);
        color = mix(o, color, clamp(0.5 - (d + w), 0.0, 1.0));
    }
    return clamp(color, vec4(0.0), vec4(1.0)) * coverage;
}
