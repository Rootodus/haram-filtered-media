use super::{App, context, draw, pipeline};
use crate::inference::run_inference_large;
use crate::protocol::{MAX_ACTIONS, SEQ_LEN};
use crate::schema::Metadata;
use crate::shared_state::{INFERENCE_RUNNING, SKIP_NEXT_INFERENCE};
use crate::tokenizer::tokenize;

use futures::future::join_all;
use ndarray::Array2;
use ort::value::Value;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tokio::task::spawn_blocking;
use wgpu::{
    CompositeAlphaMode, DeviceDescriptor, Features, Instance, Limits, MemoryHints, PowerPreference,
    PresentMode, RequestAdapterOptions, SurfaceColorSpace, SurfaceConfiguration, TextureFormat,
    TextureUsages,
};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("ML Filtered Browser"))
                .unwrap(),
        );
        self.window = Some(window.clone());

        let instance = Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("Failed to find adapter");

        let info = adapter.get_info();
        println!(
            "Using Graphics Backend: {:?} | Device: {}",
            info.backend, info.name
        );

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: Some("Primary Device"),
            required_features: Features::empty(),
            required_limits: Limits::default(),
            memory_hints: MemoryHints::Performance,
            experimental_features: Default::default(),
            trace: wgpu::Trace::Off,
        }))
        .expect("Failed to create device");

        let size = window.inner_size();
        self.viewport_size = (size.width.max(1), size.height.max(1));

        // Surface configuration
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format: TextureFormat::Rgba8UnormSrgb,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: PresentMode::Immediate,
            alpha_mode: CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: SurfaceColorSpace::Srgb,
        };
        surface.configure(&device, &config);
        self.surface = Some(surface);
        self.device = Some(device.clone());
        self.queue = Some(queue);

        // 1. Create pipelines and store them in a local stack variable
        let pipelines = pipeline::create_pipelines(&device);

        // ---------- Sampler ----------
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        self.sampler = Some(sampler.clone());

        // ---------- Mask resources (Keep in local variables first!) ----------
        let mask_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Mask Texture"),
            size: wgpu::Extent3d {
                width: size.width.max(1),
                height: size.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let mask_texture_view = mask_texture.create_view(&Default::default());

        let mask_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mask Uniform Buffer"),
            size: std::mem::size_of::<context::MaskUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mask_storage_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Mask Storage Buffer"),
            size: (std::mem::size_of::<context::ActionInstance>() * MAX_ACTIONS) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create the mask bind group using the local variables safely
        let mask_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            // Fixed struct type here
            label: Some("Mask Bind Group"),
            layout: &pipelines.mask_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: mask_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: mask_storage_buffer.as_entire_binding(),
                },
            ],
        });

        // ---------- Final uniform buffer (Keep local) ----------
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Final Uniform Buffer"),
            size: std::mem::size_of::<context::FinalUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Dummy bind group for initial state
        let dummy_texture_view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Dummy"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());

        let dummy_mask_view = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Dummy Mask"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());

        // Create final bind group using the local variables safely
        let final_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Final Bind Group"),
            layout: &pipelines.final_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&dummy_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&dummy_mask_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        // 2. NOW assign ownership to self fields at the very end
        self.mask_pipeline = Some(pipelines.mask_pipeline);
        self.pipeline = Some(pipelines.final_pipeline);
        self.mask_bind_group_layout = Some(pipelines.mask_bind_group_layout);
        self.bind_group_layout = Some(pipelines.final_bind_group_layout);

        self.mask_texture = Some(mask_texture);
        self.mask_texture_view = Some(mask_texture_view);
        self.mask_uniform_buffer = Some(mask_uniform_buffer);
        self.mask_storage_buffer = Some(mask_storage_buffer);
        self.mask_bind_group = Some(mask_bind_group);
        self.uniform_buffer = Some(uniform_buffer);
        self.bind_group = Some(final_bind_group);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                // 1. Extract and immediately CLONE the device and queue handles to free up self fields.
                // We also grab the viewport size locally so we don't borrow self inside the error blocks.
                let (device, queue, viewport_width, viewport_height) = {
                    let (Some(d), Some(q)) = (&self.device, &self.queue) else {
                        return;
                    };
                    (
                        d.clone(),
                        q.clone(),
                        self.viewport_size.0,
                        self.viewport_size.1,
                    )
                };

                // 2. Fetch the current surface texture in an isolated short-lived block.
                // This immediately ends the immutable borrow on self.surface so that downstream
                // functions can mutably borrow self safely.
                let surface_texture = {
                    let Some(surf) = &self.surface else {
                        return;
                    };
                    match surf.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(t) => t,
                        wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                        wgpu::CurrentSurfaceTexture::Timeout
                        | wgpu::CurrentSurfaceTexture::Outdated => {
                            eprintln!("Surface state non-optimal: Outdated or Timeout");
                            return;
                        }
                        wgpu::CurrentSurfaceTexture::Lost => {
                            eprintln!("Surface critical failure: Lost! Reconfiguring...");
                            let cfg = SurfaceConfiguration {
                                usage: TextureUsages::RENDER_ATTACHMENT,
                                format: TextureFormat::Rgba8UnormSrgb,
                                width: viewport_width,
                                height: viewport_height,
                                present_mode: PresentMode::Immediate,
                                alpha_mode: CompositeAlphaMode::Auto,
                                view_formats: vec![],
                                desired_maximum_frame_latency: 2,
                                color_space: SurfaceColorSpace::Srgb,
                            };
                            surf.configure(&device, &cfg);
                            return;
                        }
                        _ => return,
                    }
                };

                // ---------- Data update (inference) ----------
                let mut needs_ack = false;
                let mut actions = Vec::new();

                if let Some(frame) = self.state.get_frame_if_dirty() {
                    let should_skip = SKIP_NEXT_INFERENCE.swap(false, Ordering::AcqRel);
                    let has_active_sessions = !self.sessions.is_empty();

                    if !should_skip && has_active_sessions {
                        INFERENCE_RUNNING.store(true, Ordering::Relaxed);
                        let metadata =
                            unsafe { flatbuffers::root_unchecked::<Metadata>(&frame.buffer) };
                        let nodes = metadata.nodes().unwrap_or_default();
                        let mut full_text = String::new();
                        for i in 0..nodes.len() {
                            let node = nodes.get(i);
                            if let Some(text) = node.text() {
                                if !text.is_empty() {
                                    full_text.push_str(text);
                                    full_text.push(' ');
                                }
                            }
                        }
                        if full_text.is_empty() {
                            full_text.push_str("empty");
                        }
                        let (input_ids_vec, attention_mask_vec) = tokenize(&full_text, SEQ_LEN);
                        let ids_array = Array2::from_shape_vec((1, SEQ_LEN), input_ids_vec)
                            .expect("Failed to create input_ids array");
                        let mask_array = Array2::from_shape_vec((1, SEQ_LEN), attention_mask_vec)
                            .expect("Failed to create attention_mask array");
                        let ids_value = Value::from_array(ids_array)
                            .expect("Failed to create input_ids Value")
                            .into_dyn();
                        let mask_value = Value::from_array(mask_array)
                            .expect("Failed to create attention_mask Value")
                            .into_dyn();
                        let ids_arc = Arc::new(ids_value);
                        let mask_arc = Arc::new(mask_value);
                        let buffer_arc = Arc::clone(&frame.buffer);
                        let mut handles = Vec::with_capacity(self.sessions.len());

                        for session_arc in &self.sessions {
                            let session_clone = Arc::clone(session_arc);
                            let ids_clone = Arc::clone(&ids_arc);
                            let mask_clone = Arc::clone(&mask_arc);
                            let buffer_clone = Arc::clone(&buffer_arc);
                            let handle = spawn_blocking(move || {
                                let metadata = unsafe {
                                    flatbuffers::root_unchecked::<Metadata>(&buffer_clone)
                                };
                                let mut session_guard = session_clone.lock().unwrap();
                                run_inference_large(
                                    &mut session_guard,
                                    &ids_clone,
                                    &mask_clone,
                                    Some(&metadata),
                                )
                            });
                            handles.push(handle);
                        }
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
                            self.state.set_actions(all_actions.clone());
                        } else {
                            self.state.set_actions(Vec::new());
                        }
                        actions = all_actions;
                        INFERENCE_RUNNING.store(false, Ordering::Relaxed);
                    }

                    // Upload frame texture - completely safe now!
                    draw::upload_frame_texture(
                        &queue,
                        self,
                        frame.width,
                        frame.height,
                        &frame.pixel_data,
                    );
                    needs_ack = true;
                }

                // ---------- Begin rendering ----------
                if self.frame_texture_view.is_none() || self.mask_texture_view.is_none() {
                    return;
                }

                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                // --- Pass 1: Mask generation ---
                draw::run_mask_pass(&mut encoder, &queue, self, &actions);

                // --- Pass 2: Final composition ---
                if let Err(e) = draw::run_final_pass(&mut encoder, &queue, self, &surface_texture) {
                    eprintln!("Final pass error: {:?}", e);
                    return;
                }

                // ---------- Submit and present ----------
                queue.submit(std::iter::once(encoder.finish()));
                queue.present(surface_texture);

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
