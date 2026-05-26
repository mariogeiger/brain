// Volume raymarcher — maximum intensity projection through a 3D texture.

struct Uniforms {
    inv_view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,        // .xyz = camera world pos
    box_half: vec4<f32>,       // .xyz = volume half-extents in world space
    params: vec4<f32>,         // .x = step count (f32), .y = brain_lo, .z = brain_hi, .w = step alpha
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var vol_tex: texture_3d<f32>;
@group(0) @binding(2) var vol_samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};

// Fullscreen triangle, no vertex buffer.
@vertex
fn vs(@builtin(vertex_index) vid: u32) -> VsOut {
    var out: VsOut;
    let x = f32((vid << 1u) & 2u);
    let y = f32(vid & 2u);
    out.ndc = vec2<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0);
    out.pos = vec4<f32>(out.ndc, 0.0, 1.0);
    return out;
}

// Ray-AABB intersection. Returns vec2(t_near, t_far); t_far < t_near means miss.
fn intersect_box(ro: vec3<f32>, rd: vec3<f32>, half: vec3<f32>) -> vec2<f32> {
    let inv_d = 1.0 / rd;
    let t0 = (-half - ro) * inv_d;
    let t1 = (half - ro) * inv_d;
    let tmin = min(t0, t1);
    let tmax = max(t0, t1);
    let t_near = max(max(tmin.x, tmin.y), tmin.z);
    let t_far = min(min(tmax.x, tmax.y), tmax.z);
    return vec2<f32>(t_near, t_far);
}

// Five-stop gradient in catppuccin spirit: deep → mauve → pink → peach → yellow.
fn colormap(t: f32) -> vec3<f32> {
    let c0 = vec3<f32>(0.10, 0.10, 0.15);  // base shadow
    let c1 = vec3<f32>(0.27, 0.16, 0.42);  // deep mauve
    let c2 = vec3<f32>(0.80, 0.65, 0.97);  // mauve  #cba6f7
    let c3 = vec3<f32>(0.96, 0.76, 0.91);  // pink   #f5c2e7
    let c4 = vec3<f32>(0.98, 0.70, 0.53);  // peach  #fab387
    let c5 = vec3<f32>(0.98, 0.89, 0.69);  // yellow #f9e2af
    let s = clamp(t, 0.0, 1.0) * 5.0;
    if (s < 1.0) { return mix(c0, c1, s); }
    if (s < 2.0) { return mix(c1, c2, s - 1.0); }
    if (s < 3.0) { return mix(c2, c3, s - 2.0); }
    if (s < 4.0) { return mix(c3, c4, s - 3.0); }
    return mix(c4, c5, s - 4.0);
}

// Windowed transfer function: only voxels in the brain-tissue intensity band
// contribute. Skull/scalp (very bright on T1) and air (very dark) become
// invisible, so the brain matter emerges from inside the head silhouette.
fn transfer(v: f32, lo: f32, hi: f32, step_a: f32) -> vec4<f32> {
    if (v < lo || v > hi) {
        return vec4<f32>(0.0);
    }
    let t = (v - lo) / (hi - lo);
    // Soft fade at the band edges so we don't get sharp boundary artefacts.
    let edge = smoothstep(0.0, 0.12, t) * (1.0 - smoothstep(0.88, 1.0, t));
    return vec4<f32>(colormap(t), step_a * edge);
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let far_clip = u.inv_view_proj * vec4<f32>(in.ndc, 1.0, 1.0);
    let world_far = far_clip.xyz / far_clip.w;
    let ro = u.cam_pos.xyz;
    let rd = normalize(world_far - ro);

    let half = u.box_half.xyz;
    let bg = vec3<f32>(0.10, 0.10, 0.15);
    let t = intersect_box(ro, rd, half);
    let t_near = max(t.x, 0.0);
    let t_far = t.y;
    if (t_far < t_near) {
        let vig = 1.0 - length(in.ndc) * 0.5;
        return vec4<f32>(bg * vig, 1.0);
    }

    let n_steps_f = u.params.x;
    let n_steps = i32(n_steps_f);
    let brain_lo = u.params.y;
    let brain_hi = u.params.z;
    let step_a = u.params.w;
    let dt = (t_far - t_near) / n_steps_f;

    // Front-to-back alpha compositing.
    var col = vec3<f32>(0.0);
    var alpha = 0.0;
    var t_cur = t_near + dt * 0.5;
    for (var i = 0; i < n_steps; i = i + 1) {
        let p = ro + rd * t_cur;
        let uv = (p / half) * 0.5 + 0.5;
        let v = textureSampleLevel(vol_tex, vol_samp, uv, 0.0).r;
        let tf = transfer(v, brain_lo, brain_hi, step_a);
        let a_contrib = (1.0 - alpha) * tf.a;
        col = col + a_contrib * tf.rgb;
        alpha = alpha + a_contrib;
        if (alpha > 0.99) { break; }
        t_cur = t_cur + dt;
    }

    let out_col = col + (1.0 - alpha) * bg;
    return vec4<f32>(out_col, 1.0);
}
