// ============================================================================
// PASS 1: MASK GENERATION (Rasterize Rectangles directly to R8 Texture)
// ============================================================================

struct MaskUniforms {
    viewport_width: f32,
    viewport_height: f32,
}
@group(0) @binding(0) var<uniform> mask_view: MaskUniforms;

struct ActionInstance {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    action_type: u32, // 1 = Blur, 2 = Blackbox (using distinct values)
    _pad: vec3<u32>,
}
@group(0) @binding(1) var<storage, read> actions: array<ActionInstance>;

struct MaskVertexOutput {
    @builtin(position) position: vec4<f32>,
    // FIX: Changed from '@flat' to the correct standard WGSL interpolation modifier
    @location(0) @interpolate(flat) action_type: u32,
}

@vertex
fn vs_mask(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32
) -> MaskVertexOutput {
    let act = actions[instance_index];
    
    // Compute corner coordinates of a unit square quad based on index (0..3)
    let tx = f32(vertex_index & 1u);          // 0.0 or 1.0
    let ty = f32((vertex_index >> 1u) & 1u);   // 0.0 or 1.0

    // Interpolate coordinates in pixel space
    let px = act.x + (tx * act.width);
    let py = act.y + (ty * act.height);

    // Map pixel space [0, Viewport] to hardware Clip Space [-1, 1]
    let ndc_x = (px / mask_view.viewport_width) * 2.0 - 1.0;
    let ndc_y = 1.0 - (py / mask_view.viewport_height) * 2.0; // Invert Y for graphics standard

    var out: MaskVertexOutput;
    out.position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.action_type = act.action_type;
    return out;
}

@fragment
fn fs_mask(in: MaskVertexOutput) -> @location(0) vec4<f32> {
    // Normalize target flags into the R8 float value spectrum 
    // Type 1 (Blur) -> 0.5, Type 2 (Blackbox) -> 1.0
    if (in.action_type == 1u) {
        return vec4<f32>(0.5, 0.0, 0.0, 1.0);
    }
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}

// ============================================================================
// PASS 2 & 3: FINAL SCREEN COMPOSITING
// ============================================================================

struct FinalUniforms {
    viewport_width: f32,
    viewport_height: f32,
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var mask_tex: texture_2d<f32>; // The generated R8 mask lookup
@group(0) @binding(3) var<uniform> uniforms: FinalUniforms;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32(vertex_index & 1u) * 2.0 - 1.0;
    let y = f32((vertex_index >> 1u) & 1u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn fs_main(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    // FIX: Added 'pos' named handle variable to restore correct coordinate accessing
    let uv = pos.xy / vec2<f32>(uniforms.viewport_width, uniforms.viewport_height);
    let raw_color = textureSample(tex, samp, uv);
    
    // Look up what processing behavior this specific screen texel demands
    let mask_val = textureSample(mask_tex, samp, uv).r;

    // Fast-path evaluation: Zero masking implies base presentation
    if (mask_val < 0.1) {
        return raw_color;
    }
    
    // Mask matches Blackbox condition flag (1.0)
    if (mask_val > 0.8) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }

    // Mask matches Blur condition flag (0.5). Evaluate 9-tap texture kernel exactly once
    let offset = vec2<f32>(1.5, 1.5) / vec2<f32>(uniforms.viewport_width, uniforms.viewport_height);
    var sum = textureSampleLevel(tex, samp, uv + offset * vec2<f32>(-1.0, -1.0), 0.0);
    sum += textureSampleLevel(tex, samp, uv + offset * vec2<f32>(0.0, -1.0), 0.0);
    sum += textureSampleLevel(tex, samp, uv + offset * vec2<f32>(1.0, -1.0), 0.0);
    sum += textureSampleLevel(tex, samp, uv + offset * vec2<f32>(-1.0, 0.0), 0.0);
    sum += textureSampleLevel(tex, samp, uv + offset * vec2<f32>(0.0, 0.0), 0.0);
    sum += textureSampleLevel(tex, samp, uv + offset * vec2<f32>(1.0, 0.0), 0.0);
    sum += textureSampleLevel(tex, samp, uv + offset * vec2<f32>(-1.0, 1.0), 0.0);
    sum += textureSampleLevel(tex, samp, uv + offset * vec2<f32>(0.0, 1.0), 0.0);
    sum += textureSampleLevel(tex, samp, uv + offset * vec2<f32>(1.0, 1.0), 0.0);
    
    return sum / 9.0;
}
