# Layered Composition Renderer
ID: SPEC-RENDERER  
Status: STABLE-FOR-IMPLEMENTATION  
Depends on: ARCH-REQ, ARCH-SYS-MAP, SPEC-ML-PROC, STYLE-RUST

## 1. Pipeline Layout [RENDER-PIPELINE]
- Target: `wgpu 29.0`.
- Primitive: A single full-screen "Quad" (two triangles).
- Bind Group 0: Contains the `frame_texture` (Sampler and TextureView).
- Bind Group 1: Contains the `ActionOverlay` buffer (Coordinate masks for Blurs/Blackboxes).

## 2. Media Layer Implementation [RENDER-STREAM]
- Texture Upload: Use `queue.write_texture` with `TexelCopyTextureInfo`.
- Constraint: Must use the `Arc<[u8]>` from `SharedAppState` without copying.
- Shader: A fragment shader SHALL sample the `frame_texture` and apply pixel-discard or Gaussian-blur logic based on the `VisualAction` list.

## 3. Content Layer Implementation [RENDER-CONTENT]
- Text Rendering: Semantic replacements from `ProcessedBuffer` SHALL be rendered using `cosmic-text`.
- Overlay: The text layer IS composited over the Media Layer quad in a second render pass.

## 4. Admission Invariants
- V-Sync Policy: The `Renderer` SHALL use `PresentMode::Immediate` or `Mailbox` for performance testing, but `Fifo` is the default for user stability.
- Drop Policy: If `state.get_frame_if_dirty()` returns `None` during a `RedrawRequested` event, the `Renderer` SHALL redraw the last valid `frame_texture` to maintain UI responsiveness.
