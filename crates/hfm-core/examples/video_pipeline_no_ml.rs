use crossbeam::queue::ArrayQueue;
use hfm_core::memory::{PackedIndex, STATE_GPU_UPLOADED, STATE_INGESTED, SlotPool};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use wgpu::{
    BackendOptions, Backends, Device, DeviceDescriptor, ExperimentalFeatures, Features, Instance,
    InstanceDescriptor, InstanceFlags, Limits, MemoryBudgetThresholds, MemoryHints,
    PowerPreference, Queue, RequestAdapterOptions, Surface, SurfaceConfiguration, Trace,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

// ---- GStreamer imports ----
use gst::glib::ControlFlow;
use gst_app::AppSink;
use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video::prelude::*;

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
const VIDEO_SLOT_SIZE: usize = (WIDTH * HEIGHT * 4) as usize;
const N_V: usize = 128;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
}

const QUAD_VERTICES: [Vertex; 6] = [
    Vertex {
        position: [-1.0, -1.0],
        uv: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, -1.0],
        uv: [1.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0],
        uv: [1.0, 0.0],
    },
    Vertex {
        position: [-1.0, -1.0],
        uv: [0.0, 1.0],
    },
    Vertex {
        position: [1.0, 1.0],
        uv: [1.0, 0.0],
    },
    Vertex {
        position: [-1.0, 1.0],
        uv: [0.0, 0.0],
    },
];

struct App {
    window: Option<Arc<Window>>,
    surface: Option<Surface<'static>>,
    device: Option<Device>,
    queue: Option<Queue>,
    config: Option<SurfaceConfiguration>,
    render_pipeline: Option<wgpu::RenderPipeline>,
    vertex_buffer: Option<wgpu::Buffer>,
    texture: Option<wgpu::Texture>,
    sampler: Option<wgpu::Sampler>,
    bind_group: Option<wgpu::BindGroup>,

    pool: Option<Arc<SlotPool<VIDEO_SLOT_SIZE>>>,
    video_ingested: Option<Arc<ArrayQueue<PackedIndex>>>,
    video_gpu_upload_ready: Option<Arc<ArrayQueue<PackedIndex>>>,

    ingest_handle: Option<std::thread::JoinHandle<()>>,
    upload_handle: Option<std::thread::JoinHandle<()>>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            surface: None,
            device: None,
            queue: None,
            config: None,
            render_pipeline: None,
            vertex_buffer: None,
            texture: None,
            sampler: None,
            bind_group: None,
            pool: None,
            video_ingested: None,
            video_gpu_upload_ready: None,
            ingest_handle: None,
            upload_handle: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("Video Pipeline (No ML)")
                        .with_inner_size(winit::dpi::LogicalSize::new(WIDTH, HEIGHT)),
                )
                .unwrap(),
        );
        self.window = Some(window.clone());

        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::PRIMARY,
            flags: InstanceFlags::default(),
            memory_budget_thresholds: MemoryBudgetThresholds::default(),
            backend_options: BackendOptions::default(),
            display: None,
        });

        let surface = instance.create_surface(window).unwrap();
        self.surface = Some(surface);

        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            compatible_surface: self.surface.as_ref(),
            ..Default::default()
        }))
        .expect("No suitable GPU adapter");

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: None,
            required_features: Features::empty(),
            required_limits: Limits::default(),
            experimental_features: ExperimentalFeatures::disabled(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .expect("Failed to create device");
        self.device = Some(device);
        self.queue = Some(queue);

        let caps = self.surface.as_ref().unwrap().get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let config = SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: WIDTH,
            height: HEIGHT,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            color_space: wgpu::SurfaceColorSpace::Auto,
        };
        self.surface
            .as_ref()
            .unwrap()
            .configure(self.device.as_ref().unwrap(), &config);
        self.config = Some(config);

        // --- Texture & Sampler ---
        let texture = self
            .device
            .as_ref()
            .unwrap()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Video Texture"),
                size: wgpu::Extent3d {
                    width: WIDTH,
                    height: HEIGHT,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
        self.texture = Some(texture);

        let sampler = self
            .device
            .as_ref()
            .unwrap()
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Video Sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                ..Default::default()
            });
        self.sampler = Some(sampler);

        // --- Vertex Buffer ---
        let vertex_buffer =
            self.device
                .as_ref()
                .unwrap()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Quad Vertex Buffer"),
                    contents: bytemuck::cast_slice(&QUAD_VERTICES),
                    usage: wgpu::BufferUsages::VERTEX,
                });
        self.vertex_buffer = Some(vertex_buffer);

        // --- Shader ---
        let shader =
            self.device
                .as_ref()
                .unwrap()
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Texture Shader"),
                    source: wgpu::ShaderSource::Wgsl(
                        include_str!("shaders/texture_quad.wgsl").into(),
                    ),
                });

        // --- Bind Group Layout ---
        let bind_group_layout = self.device.as_ref().unwrap().create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Video Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            },
        );

        // --- Bind Group ---
        let texture_view = self
            .texture
            .as_ref()
            .unwrap()
            .create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group =
            self.device
                .as_ref()
                .unwrap()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Video Bind Group"),
                    layout: &bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&texture_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(
                                self.sampler.as_ref().unwrap(),
                            ),
                        },
                    ],
                });
        self.bind_group = Some(bind_group);

        // --- Pipeline Layout ---
        let pipeline_layout =
            self.device
                .as_ref()
                .unwrap()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: None,
                    bind_group_layouts: &[Some(&bind_group_layout)],
                    immediate_size: 0,
                });

        // --- Render Pipeline ---
        let render_pipeline =
            self.device
                .as_ref()
                .unwrap()
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: None,
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_main"),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &[
                                wgpu::VertexAttribute {
                                    offset: 0,
                                    shader_location: 0,
                                    format: wgpu::VertexFormat::Float32x2,
                                },
                                wgpu::VertexAttribute {
                                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                                    shader_location: 1,
                                    format: wgpu::VertexFormat::Float32x2,
                                },
                            ],
                        })],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_main"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(wgpu::BlendState::REPLACE),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    }),
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                });
        self.render_pipeline = Some(render_pipeline);

        // --- Video Pipeline (queues) ---
        let pool = Arc::new(SlotPool::<VIDEO_SLOT_SIZE>::new(N_V));
        self.pool = Some(pool.clone());

        let video_ingested = Arc::new(ArrayQueue::<PackedIndex>::new(N_V));
        let video_gpu_upload_ready = Arc::new(ArrayQueue::<PackedIndex>::new(N_V));
        self.video_ingested = Some(video_ingested.clone());
        self.video_gpu_upload_ready = Some(video_gpu_upload_ready.clone());

        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

        // ---- Ingest thread (GStreamer) ----
        let pool_ingest = pool.clone();
        let video_ingested_prod = video_ingested.clone();
        let running_ingest = running.clone();

        let ingest_handle = std::thread::spawn(move || {
            // Initialize GStreamer (must be done per thread? Actually it's global, but we can init in main and reuse.
            // We'll init here to be safe, but gst::init() can be called multiple times.
            if let Err(e) = gst::init() {
                eprintln!("[INGEST] GStreamer init failed: {}", e);
                return;
            }

            // Build pipeline
            let pipeline = gst::Pipeline::new();

            // Create elements
            let src = match gst::ElementFactory::make("filesrc")
                .property("location", "assets/video.mp4")
                .build()
            {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("[INGEST] Failed to create filesrc: {}", e);
                    return;
                }
            };

            let decodebin = match gst::ElementFactory::make("decodebin").build() {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("[INGEST] Failed to create decodebin: {}", e);
                    return;
                }
            };

            let convert = match gst::ElementFactory::make("videoconvert").build() {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("[INGEST] Failed to create videoconvert: {}", e);
                    return;
                }
            };
            let scale = match gst::ElementFactory::make("videoscale").build() {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("[INGEST] Failed to create videoscale: {}", e);
                    return;
                }
            };
            let sink = AppSink::builder()
                .caps(
                    &gst::Caps::builder("video/x-raw")
                        .field("format", "RGBA")
                        .field("width", WIDTH as i32)
                        .field("height", HEIGHT as i32)
                        .build(),
                )
                .async_(true)
                .drop(true)
                .build();
            let sink_element = sink.upcast_ref::<gst::Element>().clone();

            // Add elements to pipeline
            if let Err(e) = pipeline.add_many(&[&src, &decodebin, &convert, &scale, &sink_element])
            {
                eprintln!("[INGEST] Failed to add elements: {}", e);
                return;
            }
            // Link src -> decodebin (pad-added handles the rest)
            if let Err(e) = gst::Element::link_many(&[&src, &decodebin]) {
                eprintln!("[INGEST] Failed to link src to decodebin: {}", e);
                return;
            }
            // Link convert -> scale -> sink
            if let Err(e) = gst::Element::link_many(&[&convert, &scale, &sink_element]) {
                eprintln!("[INGEST] Failed to link convert -> scale -> sink: {}", e);
                return;
            }

            // Connect pad-added signal
            let convert_clone = convert.clone();
            decodebin.connect_pad_added(move |_, src_pad| {
                let caps = src_pad.current_caps().expect("Failed to get caps");
                let structure = caps.structure(0).expect("No structure");
                if structure.name().starts_with("video/") {
                    let sink_pad = convert_clone
                        .static_pad("sink")
                        .expect("convert has no sink pad");
                    if sink_pad.is_linked() {
                        return;
                    }
                    let src_pad = src_pad.clone();
                    if let Err(e) = src_pad.link(&sink_pad) {
                        eprintln!("[INGEST] Failed to link decodebin pad to convert: {}", e);
                    }
                }
            });

            // Set pipeline to playing
            if let Err(e) = pipeline.set_state(gst::State::Playing) {
                eprintln!("[INGEST] Failed to set pipeline to playing: {}", e);
                return;
            }

            // Bus watch for errors
            let bus = match pipeline.bus() {
                Some(b) => b,
                None => {
                    eprintln!("[INGEST] No bus");
                    return;
                }
            };
            let _watch_id = match bus.add_watch(move |_, msg| {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Error(err) => {
                        eprintln!("[INGEST] GStreamer error: {}", err.error());
                        if let Some(debug) = err.debug() {
                            eprintln!("[INGEST] Debug info: {}", debug);
                        }
                        ControlFlow::Break
                    }
                    MessageView::Eos(_) => {
                        println!("[INGEST] End of stream");
                        ControlFlow::Break
                    }
                    _ => ControlFlow::Continue,
                }
            }) {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("[INGEST] Failed to add bus watch: {}", e);
                    return;
                }
            };

            let mut frame_count = 0;

            // Pull samples
            while running_ingest.load(std::sync::atomic::Ordering::Acquire) {
                let sample = match sink.pull_sample() {
                    Ok(s) => s,
                    Err(e) => {
                        // EOS or error
                        if e.message.contains("EOS") {
                            println!("[INGEST] End of stream");
                        } else {
                            eprintln!("[INGEST] pull_sample error: {}", e.message);
                        }
                        break;
                    }
                };

                let buffer = match sample.buffer() {
                    Some(b) => b,
                    None => {
                        eprintln!("[INGEST] No buffer");
                        continue;
                    }
                };
                let caps = match sample.caps() {
                    Some(c) => c,
                    None => {
                        eprintln!("[INGEST] No caps");
                        continue;
                    }
                };
                let structure = match caps.structure(0) {
                    Some(s) => s,
                    None => {
                        eprintln!("[INGEST] No structure");
                        continue;
                    }
                };

                let format = structure.get::<&str>("format").unwrap_or("unknown");

                if format != "RGBA" {
                    eprintln!("[INGEST] Expected RGBA but got {}", format);
                    continue;
                }

                // Map buffer
                let map = match buffer.map_readable() {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("[INGEST] Failed to map buffer: {}", e);
                        continue;
                    }
                };

                // Buffer is already RGBA and at target resolution (960x540)
                let rgba = map.as_slice().to_vec();

                // Diagnostic for first frame
                if frame_count == 0 {
                    println!(
                        "[INGEST] First frame RGBA buffer len: {}, expected: {}",
                        rgba.len(),
                        WIDTH as usize * HEIGHT as usize * 4
                    );
                    if rgba.len() >= 16 {
                        println!("[INGEST] First 16 RGBA bytes: {:02x?}", &rgba[..16]);
                    }
                }

                // Claim a slot and push to queue (no resize needed)
                if let Some(packed) = pool_ingest.try_claim() {
                    pool_ingest.with_payload_mut(packed, |payload| {
                        payload.copy_from_slice(&rgba);
                    });
                    while let Err(_) = video_ingested_prod.push(packed) {
                        std::thread::sleep(std::time::Duration::from_micros(100));
                    }
                } else {
                    // No free slot; drop frame
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }

                frame_count += 1;
                if frame_count % 10 == 0 {
                    println!("[INGEST] Decoded frame #{}", frame_count);
                }
            }

            // Clean up
            pipeline.set_state(gst::State::Null).ok();
            println!(
                "[INGEST] Exiting ingest thread. Total frames decoded: {}",
                frame_count
            );
        });

        self.ingest_handle = Some(ingest_handle);

        // ---- Upload thread (consumes from video_ingested) ----
        let pool_upload = pool.clone();
        let video_ingested_cons = video_ingested.clone();
        let video_gpu_upload_ready_prod = video_gpu_upload_ready.clone();
        let device_upload = self.device.as_ref().unwrap().clone();
        let queue_upload = self.queue.as_ref().unwrap().clone();
        let texture_upload = self.texture.as_ref().unwrap().clone();
        let running_upload = running.clone();

        let upload_handle = std::thread::spawn(move || {
            while running_upload.load(std::sync::atomic::Ordering::Acquire) {
                if let Some(packed) = video_ingested_cons.pop() {
                    let payload = pool_upload.with_payload_mut(packed, |p| p.to_vec());
                    let staging =
                        device_upload.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Frame Staging"),
                            contents: &payload,
                            usage: wgpu::BufferUsages::COPY_SRC,
                        });
                    let mut encoder =
                        device_upload.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: Some("Copy Encoder"),
                        });
                    encoder.copy_buffer_to_texture(
                        wgpu::TexelCopyBufferInfo {
                            buffer: &staging,
                            layout: wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(4 * WIDTH),
                                rows_per_image: Some(HEIGHT),
                            },
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture_upload,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d {
                            width: WIDTH,
                            height: HEIGHT,
                            depth_or_array_layers: 1,
                        },
                    );
                    queue_upload.submit(Some(encoder.finish()));
                    pool_upload
                        .transition_state(packed, STATE_INGESTED, STATE_GPU_UPLOADED)
                        .unwrap();
                    video_gpu_upload_ready_prod.push(packed).unwrap();
                } else {
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
            }
        });
        self.upload_handle = Some(upload_handle);

        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                // ---- Render ----
                let surface = self.surface.as_ref().unwrap();
                let device = self.device.as_ref().unwrap();
                let queue = self.queue.as_ref().unwrap();
                let render_pipeline = self.render_pipeline.as_ref().unwrap();
                let vertex_buffer = self.vertex_buffer.as_ref().unwrap();
                let bind_group = self.bind_group.as_ref().unwrap();
                let pool = self.pool.as_ref().unwrap();
                let video_gpu_upload_ready = self.video_gpu_upload_ready.as_ref().unwrap();

                let ready_count = video_gpu_upload_ready.len();
                if ready_count < 1 {
                    self.window.as_ref().unwrap().request_redraw();
                    return;
                }

                match surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(frame) => {
                        let view = frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        let mut encoder =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("Render Encoder"),
                            });

                        {
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

                            pass.set_pipeline(render_pipeline);
                            pass.set_bind_group(0, bind_group, &[]);
                            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                            pass.draw(0..6, 0..1);
                        }

                        queue.submit(Some(encoder.finish()));
                        queue.present(frame);

                        // After presenting, pop the frame that was used and release its slot
                        if let Some(packed) = video_gpu_upload_ready.pop() {
                            pool.release_video(packed);
                        }
                    }
                    wgpu::CurrentSurfaceTexture::Timeout
                    | wgpu::CurrentSurfaceTexture::Occluded => {
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                    }
                    wgpu::CurrentSurfaceTexture::Outdated => {
                        let size = self.window.as_ref().unwrap().inner_size();
                        if size.width > 0 && size.height > 0 {
                            let config = self.config.as_mut().unwrap();
                            config.width = size.width;
                            config.height = size.height;
                            device
                                .poll(wgpu::PollType::Wait {
                                    submission_index: None,
                                    timeout: None,
                                })
                                .expect("Failed to poll GPU before reconfiguring (Outdated)");
                            surface.configure(device, config);
                        }
                        if let Some(window) = self.window.as_ref() {
                            window.request_redraw();
                        }
                    }
                    wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                        let view = frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default());
                        let mut encoder =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("Render Encoder"),
                            });

                        {
                            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
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

                            pass.set_pipeline(render_pipeline);
                            pass.set_bind_group(0, bind_group, &[]);
                            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
                            pass.draw(0..6, 0..1);
                        }

                        queue.submit(Some(encoder.finish()));
                        queue.present(frame);

                        if let Some(packed) = video_gpu_upload_ready.pop() {
                            pool.release_video(packed);
                        }

                        let size = self.window.as_ref().unwrap().inner_size();
                        if size.width > 0 && size.height > 0 {
                            let config = self.config.as_mut().unwrap();
                            config.width = size.width;
                            config.height = size.height;
                            device
                                .poll(wgpu::PollType::Wait {
                                    submission_index: None,
                                    timeout: None,
                                })
                                .expect("Failed to poll GPU before reconfiguring (Suboptimal)");
                            surface.configure(device, config);
                        }
                    }
                    wgpu::CurrentSurfaceTexture::Lost => eprintln!("Surface lost"),
                    wgpu::CurrentSurfaceTexture::Validation => eprintln!("Validation error"),
                }

                self.window.as_ref().unwrap().request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

fn main() {
    // Initialize GStreamer globally (once)
    if let Err(e) = gst::init() {
        eprintln!("GStreamer init failed: {}", e);
        return;
    }

    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
