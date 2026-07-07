// ============================================================================
// PASS 1: MASK GENERATION
// ============================================================================

struct MaskUniforms {
    texture_size: vec2<f32>,
}
@group(0) @binding(0) var<uniform> mask_view: MaskUniforms;

// We pack the layout into 2 sequential 16-byte blocks (Total = 32 bytes)
// This directly mirrors your Rust #[repr(C)] layout byte-for-byte!
struct ActionInstance {
    rect: vec4<f32>,       // x, y, width, height (16 bytes)
    metadata: vec4<u32>,       // action_type, pad0, pad1, pad2 (16 bytes)
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

    // Unpack variables directly from the clean hardware slots
    let act_x = act.rect.x;
    let act_y = act.rect.y;
    let act_width = act.rect.z;
    let act_height = act.rect.w;
    let act_type = act.metadata.x;

    // Standard 4-vertex quad winding binary sequence for TriangleStrip
    let tx = f32(vertex_index == 1u || vertex_index == 3u);
    let ty = f32(vertex_index >= 2u);

    // Map using clean, verified percentage bounds (0.0 to 1.0)
    let pct_x = act_x + (tx * act_width);
    let pct_y = act_y + (ty * act_height);
    let norm_pos = vec2<f32>(pct_x, pct_y);

    // Transform directly into native NDC screen clip-space
    let ndc = norm_pos * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0);

    var out: MaskVertexOutput;
    out.position = vec4<f32>(ndc, 0.0, 1.0);
    out.action_type = act_type;
    return out;
}

// FIXED: Reverted output signature back to f32 to match your exact pipeline channel targets!
@fragment
fn fs_mask(in: MaskVertexOutput) -> @location(0) f32 {
    // if in.action_type == 1u {
    //     return 0.5; // Red channel target value for Blur
    // }
    return 1.0; // Red channel target value for Blackbox
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
@group(0) @binding(4) var<storage, read> debug_actions: array<ActionInstance>;

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

    // ============================================================================
    // DIAGNOSTIC AREA: Top-Left Corner (Bypasses driver loop unroll bugs)
    // ============================================================================
    if pos.x < 100.0 && pos.y < 100.0 {
        // Inspect the first action struct layout explicitly 
        let first_action = debug_actions[0];
        let check_x = first_action.rect.x;
        let check_w = first_action.rect.z;

        if check_x == 0.0 && check_w == 0.0 {
            return vec4<f32>(1.0, 0.0, 0.0, 1.0); // Solid RED = Buffer unwritten
        }
        if check_x > 1.0 || check_w > 1.0 {
            return vec4<f32>(0.0, 0.0, 1.0, 1.0); // Solid BLUE = Memory layout mismatch
        }
        return vec4<f32>(0.0, 1.0, 0.0, 1.0);     // Solid GREEN = Stride match successful!
    }

    let mask_uv = (window_uv * uniforms.viewport_size) / uniforms.texture_size;
    let mask_val = textureSample(mask_tex, samp, mask_uv).r;

    // Fast-path: no mask targeting active
    if mask_val < 0.1 {
        return raw_color;
    }

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
