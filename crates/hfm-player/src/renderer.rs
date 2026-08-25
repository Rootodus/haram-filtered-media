//! Renderer with egui overlay.
//!
//! This module orchestrates video rendering and egui UI overlay.
//! It delegates video texture/quad handling to `video` and egui
//! state/renderer to `egui_overlay`.

mod egui_overlay;
mod video;

use crate::gui::{AppState, Bridge};
use parking_lot::Mutex;
use std::sync::Arc;
use wgpu::{
    BackendOptions, Backends, Device, DeviceDescriptor, ExperimentalFeatures, Features, Instance,
    InstanceDescriptor, InstanceFlags, Limits, MemoryBudgetThresholds, MemoryHints,
    PowerPreference, Queue, RequestAdapterOptions, Surface, SurfaceConfiguration, Trace,
};
use winit::event::WindowEvent;
use winit::window::Window;

pub use video::QUAD_VERTICES;
pub use video::Vertex;

/// Main renderer that combines video and egui overlay.
pub struct Renderer {
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    config: SurfaceConfiguration,
    video: video::VideoRenderer,
    egui: egui_overlay::EguiOverlay,
    window: Arc<Window>,
}

impl Renderer {
    /// Create a new renderer instance.
    pub async fn new(window: Arc<Window>) -> Self {
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::PRIMARY,
            flags: InstanceFlags::default(),
            memory_budget_thresholds: MemoryBudgetThresholds::default(),
            backend_options: BackendOptions::default(),
            display: None,
        });

        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .expect("No suitable GPU adapter");

        let (device, queue) = adapter
            .request_device(&DeviceDescriptor {
                label: None,
                required_features: Features::empty(),
                required_limits: Limits::default(),
                experimental_features: ExperimentalFeatures::disabled(),
                memory_hints: MemoryHints::Performance,
                trace: Trace::Off,
            })
            .await
            .expect("Failed to create device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);
        let size = window.inner_size();
        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let video = video::VideoRenderer::new(&device, format);
        let egui = egui_overlay::EguiOverlay::new(window.clone(), &device, format);

        Self {
            surface,
            device,
            queue,
            config,
            video,
            egui,
            window,
        }
    }

    /// Forward window events to egui.
    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        self.egui.handle_window_event(event);
    }

    /// Resize the surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        if width > 0 && height > 0 {
            self.config.width = width;
            self.config.height = height;
            let _ = self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
            self.surface.configure(&self.device, &self.config);
        }
    }

    /// Render a frame: upload video data (if any), run egui, and present.
    pub fn render(
        &mut self,
        frame_data: Option<Vec<u8>>,
        state: Arc<Mutex<AppState>>,
        bridge: &Bridge,
    ) {
        let has_frame = frame_data.is_some();

        // --- 1. Upload video frame (if any) ---
        if let Some(data) = frame_data {
            self.video.upload_frame(&data, &self.device, &self.queue);
        }

        // --- 2. Run egui frame ---
        let full_output = self.egui.begin_frame(state.clone(), bridge);

        // --- Extract panel height from the state (already updated by ui() ) ---
        let panel_height_pts = state.lock().bottom_panel_height;
        let scale_factor = self.window.scale_factor() as f32;
        let panel_height_px = (panel_height_pts * scale_factor).round() as u32;
        let visible_height = self.config.height.saturating_sub(panel_height_px);

        // --- 3. Apply texture deltas ---
        let mut textures_delta = full_output.textures_delta;
        self.egui
            .update_textures(&self.device, &self.queue, &textures_delta);
        textures_delta.set.clear();
        textures_delta.free.clear();

        // --- 4. Tessellate ---
        let (clipped_primitives, screen_descriptor) = self.egui.tessellate(full_output.shapes);

        // --- 5. Create encoder and update egui buffers ---
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        self.egui.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &clipped_primitives,
            &screen_descriptor,
        );

        // --- 6. Render pass ---
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => {
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                {
                    let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.0,
                                    g: 0.0,
                                    b: 0.0,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });

                    // Extend lifetime to 'static for egui-wgpu
                    let mut pass = pass.forget_lifetime();

                    // ---- Set viewport and scissor for video ----
                    if visible_height > 0 {
                        pass.set_viewport(
                            0.0,
                            0.0,
                            self.config.width as f32,
                            visible_height as f32,
                            0.0,
                            1.0,
                        );
                        pass.set_scissor_rect(0, 0, self.config.width, visible_height);
                    }

                    // Draw video quad
                    if has_frame {
                        self.video.draw(&mut pass);
                    }

                    // ---- Reset for egui (full window) ----
                    pass.set_viewport(
                        0.0,
                        0.0,
                        self.config.width as f32,
                        self.config.height as f32,
                        0.0,
                        1.0,
                    );
                    pass.set_scissor_rect(0, 0, self.config.width, self.config.height);

                    // Draw egui overlay (bottom panel will be on top)
                    self.egui
                        .render(&mut pass, &clipped_primitives, &screen_descriptor);
                }

                self.queue.submit(Some(encoder.finish()));
                self.queue.present(frame);
            }
            _ => {}
        }
    }
}
