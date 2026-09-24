struct Globals {
    screen: vec2<f32>,
    camera: vec2<f32>,
    zoom: f32,
    _pad: f32,
    _pad2: vec2<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

struct Instance {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
    @location(4) color: vec4<f32>,
    // bit 0: screen space (else world space, camera and zoom apply)
    // bit 1: silhouette (keep only the texture's alpha; colour comes from `color`)
    // bit 2: masked (texture colours ANDed with the 5-6-5 mask in bits 16-31)
    // bit 3: filter (drawn with the multiplying pipeline: the shape multiplies what
    //        is under it by `color`)
    @location(5) flags: u32,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) flags: u32,
};

@vertex
fn vs(@builtin(vertex_index) vi: u32, i: Instance) -> VOut {
    let corner = vec2<f32>(f32(vi & 1u), f32((vi >> 1u) & 1u));
    var p = i.pos + corner * i.size;
    if ((i.flags & 1u) == 0u) {
        p = (p - g.camera) * g.zoom;
    }
    var out: VOut;
    out.clip = vec4<f32>(p.x / g.screen.x * 2.0 - 1.0, 1.0 - p.y / g.screen.y * 2.0, 0.0, 1.0);
    out.uv = mix(i.uv0, i.uv1, corner);
    out.color = i.color;
    out.flags = i.flags;
    return out;
}

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> {
    let t = textureSample(atlas, atlas_sampler, v.uv);
    var c = t * v.color;
    if ((v.flags & 2u) != 0u) {
        c = vec4<f32>(v.color.rgb, t.a * v.color.a);
    }
    if (c.a < 0.004) {
        discard;
    }
    if ((v.flags & 4u) != 0u) {
        let m = v.flags >> 16u;
        let cr = u32(round(c.r * 31.0)) & ((m >> 11u) & 31u);
        let cg = u32(round(c.g * 63.0)) & ((m >> 5u) & 63u);
        let cb = u32(round(c.b * 31.0)) & (m & 31u);
        c = vec4<f32>(f32(cr) / 31.0, f32(cg) / 63.0, f32(cb) / 31.0, c.a);
    }
    if ((v.flags & 8u) != 0u) {
        // The multiplying pipeline: output the factor, faded out where the shape is.
        c = vec4<f32>(mix(vec3<f32>(1.0), v.color.rgb, t.a), 1.0);
    }
    return c;
}
