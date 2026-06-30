use crate::inference::{run_inference, run_inference_large};
use crate::parser::dom_to_tensor;
use crate::schema::Metadata;
use crate::state::{INFERENCE_RUNNING, SKIP_NEXT_INFERENCE, SharedAppState};

use futures::future::join_all;
use ort::session::Session;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tokio::task::spawn_blocking;

use wgpu::{
    Color, CommandEncoderDescriptor, Device, Extent3d, Instance, LoadOp, Operations, Origin3d,
    Queue, RenderPassColorAttachment, RenderPassDescriptor, StoreOp, Surface, SurfaceConfiguration,
    TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

pub struct App {
    pub window: Option<Arc<Window>>,
    pub surface: Option<Surface<'static>>,
    pub device: Option<Device>,
    pub queue: Option<Queue>,
    pub state: Arc<SharedAppState>,
    pub frame_texture: Option<Texture>,
    pub sessions: Vec<Arc<Mutex<Session>>>, // multiple models, each in a mutex
}

impl App {
    pub fn new(state: Arc<SharedAppState>, sessions: Vec<Arc<Mutex<Session>>>) -> Self {
        Self {
            window: None,
            surface: None,
            device: None,
            queue: None,
            state,
            frame_texture: None,
            sessions,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("Native Runtime"))
                .unwrap(),
        );
        self.window = Some(window.clone());

        let instance = Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .expect("Failed to find adapter");

        let info = adapter.get_info();
        println!(
            "Using Graphics Backend: {:?} | Device: {}",
            info.backend, info.name
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Primary Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: Default::default(),
            trace: wgpu::Trace::Off,
        }))
        .expect("Failed to create device");

        let size = window.inner_size();
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: TextureFormat::Rgba8UnormSrgb,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Immediate,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);
        self.surface = Some(surface);
        self.device = Some(device);
        self.queue = Some(queue);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let (Some(device), Some(queue), Some(surface)) =
                    (&self.device, &self.queue, &self.surface)
                else {
                    return;
                };

                let mut needs_ack = false;

                // --- PHASE 1: Data Update ---
                if let Some(frame) = self.state.get_frame_if_dirty() {
                    let should_skip = SKIP_NEXT_INFERENCE.swap(false, Ordering::AcqRel);
                    if !should_skip && !self.sessions.is_empty() {
                        INFERENCE_RUNNING.store(true, Ordering::Relaxed);

                        // Parse DOM to tensor
                        let metadata =
                            unsafe { flatbuffers::root_unchecked::<Metadata>(&frame.buffer) };
                        let max_nodes = 256;
                        let feature_dim = 410;
                        let tensor = dom_to_tensor(&metadata, max_nodes, feature_dim);

                        // Spawn one task per model
                        let mut handles = Vec::with_capacity(self.sessions.len());
                        for session_arc in &self.sessions {
                            let session_clone = Arc::clone(session_arc);
                            let tensor_clone = Arc::clone(&tensor);
                            // For large model, we ignore the tensor; we'll test with dummy input.
                            // If you want to feed the parser's tensor, you would reshape it.
                            let handle = spawn_blocking(move || {
                                let mut session_guard = session_clone.lock().unwrap();
                                // Choose which inference function to call:
                                // For large model, use run_inference_large
                                run_inference_large(&mut session_guard)
                                // For small model, keep old call:
                                // run_inference(
                                //     &mut session_guard,
                                //     &tensor_clone,
                                //     (max_nodes, feature_dim),
                                // )
                            });
                            handles.push(handle);
                        }

                        // Wait for all tasks and collect results
                        let mut all_actions = Vec::new();
                        let results = pollster::block_on(join_all(handles));
                        for res in results {
                            match res {
                                Ok(Ok(actions)) => all_actions.extend(actions),
                                Ok(Err(e)) => eprintln!("Inference error: {}", e),
                                Err(e) => eprintln!("Task panicked: {}", e),
                            }
                        }

                        if !all_actions.is_empty() {
                            println!("Total actions produced: {}", all_actions.len());
                        }

                        INFERENCE_RUNNING.store(false, Ordering::Relaxed);
                    }

                    // --- Texture upload ---
                    let texture_size = Extent3d {
                        width: frame.width,
                        height: frame.height,
                        depth_or_array_layers: 1,
                    };

                    if self.frame_texture.as_ref().map_or(true, |t| {
                        t.width() != frame.width || t.height() != frame.height
                    }) {
                        self.frame_texture = Some(device.create_texture(&TextureDescriptor {
                            label: Some("Frame Texture"),
                            size: texture_size,
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: TextureDimension::D2,
                            format: TextureFormat::Rgba8UnormSrgb,
                            usage: TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING,
                            view_formats: &[],
                        }));
                    }

                    queue.write_texture(
                        TexelCopyTextureInfo {
                            texture: self.frame_texture.as_ref().unwrap(),
                            mip_level: 0,
                            origin: Origin3d::ZERO,
                            aspect: TextureAspect::All,
                        },
                        &frame.pixel_data,
                        TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(4 * frame.width),
                            rows_per_image: Some(frame.height),
                        },
                        texture_size,
                    );

                    if let Ok(mut color) = self.state.clear_color.lock() {
                        let pulse = (frame.timestamp % 1000) as f64 / 1000.0;
                        *color = Color {
                            r: 0.1,
                            g: pulse,
                            b: 0.3,
                            a: 1.0,
                        };
                    }
                    needs_ack = true;
                }

                // --- PHASE 2: Render Pass (unchanged) ---
                let surface_texture = match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t) => t,
                    wgpu::CurrentSurfaceTexture::Outdated
                    | wgpu::CurrentSurfaceTexture::Timeout => return,
                    _ => {
                        event_loop.exit();
                        return;
                    }
                };

                let view = surface_texture.texture.create_view(&Default::default());
                let mut encoder =
                    device.create_command_encoder(&CommandEncoderDescriptor { label: None });
                let current_color = *self.state.clear_color.lock().unwrap();

                {
                    let _render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                        label: Some("Clear Pass"),
                        color_attachments: &[Some(RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: Operations {
                                load: LoadOp::Clear(current_color),
                                store: StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        ..Default::default()
                    });
                }

                queue.submit(std::iter::once(encoder.finish()));
                surface_texture.present();

                if needs_ack {
                    let _ = self.state.ack_sender.try_send(());
                }
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}
