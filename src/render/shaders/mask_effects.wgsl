// ============================================================================
// PASS 1: MASK GENERATION
// ============================================================================

struct MaskUniforms {
    texture_size: vec2<f32>,
}
@group(0) @binding(0) var<uniform> mask_view: MaskUniforms;

struct ActionInstance {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    action_type: u32, // 1 = Blur, 2 = Blackbox
    _pad: array<u32, 3>,
}

@group(0) @binding(1) var<storage, read> actions: array<ActionInstance>;

struct MaskVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) action_type: u32,
}

@vertex
fn vs_mask(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32
) -> MaskVertexOutput {
    let act = actions[instance_index];
    let tx = f32(vertex_index == 1u || vertex_index == 3u);
    let ty = f32(vertex_index >= 2u);

    let px = act.x + (tx * act.width);
    let py = act.y + (ty * act.height);

    let pixel_pos = vec2<f32>(px, py);
    let ndc = (pixel_pos / mask_view.texture_size) * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);

    var out: MaskVertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.action_type = act.action_type;
    return out;
}

@fragment
fn fs_mask(in: MaskVertexOutput) -> @location(0) f32 {
    if in.action_type == 1u {
        return 0.5;
    }
    return 1.0;
}

// ============================================================================
// PASS 2 & 3: FINAL SCREEN COMPOSITING
// ============================================================================

struct FinalUniforms {
    texture_size: vec2<f32>,
    viewport_size: vec2<f32>,
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var mask_tex: texture_2d<f32>; 
@group(0) @binding(3) var<uniform> uniforms: FinalUniforms;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(vertex_index & 1u) * 2.0 - 1.0;
    let y = f32((vertex_index >> 1u) & 1u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let window_uv = pos.xy / uniforms.viewport_size;
    let raw_color = textureSample(tex, samp, window_uv);
    let mask_uv = (window_uv * uniforms.viewport_size) / uniforms.texture_size;
    let mask_val = textureSample(mask_tex, samp, mask_uv).r;

    // Fast-path: no mask targeting active
    if mask_val < 0.1 {
        return raw_color;
    }

    // ============================================================================
    // DEBUG TOGGLE: Uncomment the line below to turn masked regions bright red
    // ============================================================================
    // return vec4<f32>(1.0, 0.0, 0.0, 1.0); 

    // Blackbox Action applied (mask clear value = 1.0)
    if mask_val > 0.8 {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    // 9-tap blur alignment (mask clear value = 0.5)
    let offset_px = 1.5 / uniforms.texture_size;
    var sum = vec4<f32>(0.0);
    let grid = array<vec2<f32>, 9>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(0.0, -1.0), vec2<f32>(1.0, -1.0),
        vec2<f32>(-1.0, 0.0), vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0),
        vec2<f32>(-1.0, 1.0), vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 1.0),
    );

    for (var i = 0u; i < 9u; i = i + 1u) {
        let sample_uv = mask_uv + offset_px * grid[i];
        sum = sum + textureSampleLevel(tex, samp, sample_uv, 0.0);
    }

    return sum / 9.0;
}
