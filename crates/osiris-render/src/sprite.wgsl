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
    // bit 4: smooth (bilinear filtering)
    // bit 5: sharp (nearest within a texel, blended only across a texel edge that
    //        falls inside a device pixel)
    @location(5) flags: u32,
};

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) flags: u32,
    // The image's corners in the atlas, so sampling never strays into its neighbours.
    @location(3) @interpolate(flat) lo: vec2<f32>,
    @location(4) @interpolate(flat) hi: vec2<f32>,
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
    out.lo = min(i.uv0, i.uv1);
    out.hi = max(i.uv0, i.uv1);
    return out;
}

// Texel `t` of the atlas, kept inside the image's texels `a`..`b`.
fn texel(t: vec2<i32>, a: vec2<i32>, b: vec2<i32>) -> vec4<f32> {
    return textureLoad(atlas, clamp(t, a, b), 0);
}

// Bilinear filtering within the image, weighting colours by their alpha so the
// transparent edges don't darken it. `k` is device pixels per texel; above 0 the
// blend is squeezed into the device pixel a texel edge falls in (sharp scaling).
fn sample_smooth(v: VOut, k: vec2<f32>) -> vec4<f32> {
    let dims = vec2<f32>(textureDimensions(atlas));
    let a = vec2<i32>(floor(v.lo * dims + 0.5));
    let b = max(vec2<i32>(ceil(v.hi * dims - 0.5)) - 1, a);
    let p = v.uv * dims - 0.5;
    var f = fract(p);
    if (k.x > 0.0) {
        f = clamp((f - 0.5) * k + 0.5, vec2<f32>(0.0), vec2<f32>(1.0));
    }
    let t = vec2<i32>(floor(p));
    let c00 = texel(t, a, b);
    let c10 = texel(t + vec2<i32>(1, 0), a, b);
    let c01 = texel(t + vec2<i32>(0, 1), a, b);
    let c11 = texel(t + vec2<i32>(1, 1), a, b);
    let pm = mix(mix(vec4<f32>(c00.rgb * c00.a, c00.a), vec4<f32>(c10.rgb * c10.a, c10.a), f.x), mix(vec4<f32>(c01.rgb * c01.a, c01.a), vec4<f32>(c11.rgb * c11.a, c11.a), f.x), f.y);
    if (pm.a <= 0.0) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(pm.rgb / pm.a, pm.a);
}

@fragment
fn fs(v: VOut) -> @location(0) vec4<f32> {
    // Nearest sampling, kept half a texel inside the image: at fractional scales an
    // edge pixel could otherwise land in the transparent gap beside it.
    let half = 0.5 / vec2<f32>(textureDimensions(atlas));
    var t = textureSample(atlas, atlas_sampler, clamp(v.uv, v.lo + half, max(v.hi - half, v.lo + half)));
    // Device pixels per texel, worked out here where control flow is still uniform.
    let k = 1.0 / max(fwidth(v.uv * vec2<f32>(textureDimensions(atlas))), vec2<f32>(1e-6));
    if ((v.flags & 32u) != 0u) {
        t = sample_smooth(v, max(k, vec2<f32>(1.0)));
    } else if ((v.flags & 16u) != 0u) {
        t = sample_smooth(v, vec2<f32>(0.0));
    }
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
