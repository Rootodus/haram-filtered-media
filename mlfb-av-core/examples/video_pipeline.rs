use crossbeam::queue::ArrayQueue;
use gstreamer::prelude::*;
use mlfb_av_core::filter::VideoFilter;
use mlfb_av_core::memory::{
    PackedIndex, STATE_GPU_UPLOADED, STATE_INGESTED, STATE_ML_COMMITTED, SlotPool,
};
use mlfb_av_core::ml::PeopleSegFilter;
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

const WIDTH: u32 = 960; // 960 % 64 == 0
const HEIGHT: u32 = 540; // keep 16:9 aspect
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
    video_ml_ready: Option<Arc<ArrayQueue<PackedIndex>>>,
    video_gpu_upload_ready: Option<Arc<ArrayQueue<PackedIndex>>>,

    ingest_handle: Option<std::thread::JoinHandle<()>>,
    ml_handle: Option<std::thread::JoinHandle<()>>,
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
            video_ml_ready: None,
            video_gpu_upload_ready: None,
            ingest_handle: None,
            ml_handle: None,
            upload_handle: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // --- UNet People Segmentation ---
        let model = Arc::new(
            PeopleSegFilter::new("models/peopleseg_1x3x384x640.onnx")
                .expect("Failed to load model"),
        );

        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("Video Pipeline Test")
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

        // --- Video Pipeline ---
        let pool = Arc::new(SlotPool::<VIDEO_SLOT_SIZE>::new(N_V));
        self.pool = Some(pool.clone());

        let video_ingested = Arc::new(ArrayQueue::<PackedIndex>::new(N_V));
        let video_ml_ready = Arc::new(ArrayQueue::<PackedIndex>::new(N_V));
        let video_gpu_upload_ready = Arc::new(ArrayQueue::<PackedIndex>::new(N_V));
        self.video_ingested = Some(video_ingested.clone());
        self.video_ml_ready = Some(video_ml_ready.clone());
        self.video_gpu_upload_ready = Some(video_gpu_upload_ready.clone());

        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

        // --- Ingest thread (GStreamer video decoder) ---
        let pool_ingest = pool.clone();
        let video_ingested_prod = video_ingested.clone();
        let running_ingest = running.clone();

        println!("[INGEST] Setting GST_DEBUG 3...");
        // Enable GStreamer debug logs to see where it hangs.
        unsafe {
            std::env::set_var("GST_DEBUG", "3");
        }
        println!("[INGEST] GST_DEBUG set to 3, initializing GStreamer...");
        if let Err(e) = gstreamer::init() {
            eprintln!("GStreamer init failed on main thread: {}", e);
            return;
        }
        println!("[MAIN] GStreamer initialized successfully on main thread.");

        let ingest_handle = std::thread::spawn(move || {
            println!("[INGEST] Formatting str...");

            let pipeline_str = format!(
                "videotestsrc pattern=ball ! videoconvert ! videoscale ! \
             video/x-raw,format=RGBA,width={},height={} ! appsink name=sink sync=false",
                WIDTH, HEIGHT
            );

            println!("[INGEST] Building pipeline: {}", pipeline_str);

            unsafe {
                std::env::set_var("GST_DEBUG", "GST_PIPELINE:4");
                std::env::set_var("GST_DEBUG_NO_COLOR", "1");
            }
            println!("[INGEST] GST_DEBUG set to GST_PIPELINE:4");

            println!("[INGEST] Parsing pipeline with gst::parse::launch...");

            // FIX: Downcast the launched element straight into a Pipeline type container
            let pipeline = match gstreamer::parse::launch(&pipeline_str) {
                Ok(p) => {
                    println!("[INGEST] Pipeline parsed successfully.");
                    p.downcast::<gstreamer::Pipeline>()
                        .expect("Launched element is not a top-level Pipeline object")
                }
                Err(e) => {
                    eprintln!("[INGEST] Pipeline parse error: {}", e);
                    panic!("Failed to parse pipeline");
                }
            };

            println!("[INGEST] Pipeline built and added.");

            // --- Add bus watch to capture errors ---
            let bus = pipeline.bus().unwrap();
            let _watch = bus
                .add_watch(|_, msg| {
                    use gstreamer::MessageView;
                    match msg.view() {
                        MessageView::Error(err) => {
                            eprintln!(
                                "[GST ERROR] {}: {}",
                                err.src()
                                    .as_ref()
                                    // FIX: Use .to_string() to convert the GString into an owned String
                                    .map(|s| s.name().to_string())
                                    .unwrap_or_else(|| "unknown".to_string()),
                                err.error()
                            );
                        }
                        MessageView::Warning(warn) => {
                            eprintln!(
                                "[GST WARNING] {}: {}",
                                warn.src()
                                    .as_ref()
                                    // FIX: Use .to_string() to convert the GString into an owned String
                                    .map(|s| s.name().to_string())
                                    .unwrap_or_else(|| "unknown".to_string()),
                                warn.error()
                            );
                        }
                        MessageView::Eos(_) => {
                            println!("[GST] End of stream.");
                        }
                        _ => {}
                    }
                    // FIX: Use the standard library ControlFlow mapping explicitly
                    gstreamer::glib::ControlFlow::Continue
                })
                .unwrap();

            println!("[INGEST] Bus watch installed.");

            let appsink = pipeline
                .by_name("sink")
                .expect("appsink not found")
                .downcast::<gstreamer_app::AppSink>()
                .expect("sink is not an appsink");
            appsink.set_property("emit-signals", false);
            println!("[INGEST] Appsink configured.");
            println!("[INGEST] Setting pipeline to Playing...");
            let state_res = pipeline.set_state(gstreamer::State::Playing);
            println!("[INGEST] Set state result: {:?}", state_res);

            // FIX: Unpack the Result properly instead of matching against State directly
            match pipeline.set_state(gstreamer::State::Playing) {
                Ok(success) => {
                    println!(
                        "[INGEST] State transition initiated successfully: {:?}",
                        success
                    );
                }
                Err(err) => {
                    eprintln!(
                        "[INGEST] Critical: Failed to set pipeline to Playing: {:?}",
                        err
                    );
                    panic!("GStreamer pipeline failed to start");
                }
            }

            let mut frame_count = 0;

            while running_ingest.load(std::sync::atomic::Ordering::Acquire) {
                match appsink.pull_sample() {
                    Ok(sample) => {
                        frame_count += 1;
                        if frame_count % 10 == 0 {
                            println!("[INGEST] Successfully pulled frame #{}", frame_count);
                        }

                        // No resolution checks or caps unpacking variables needed anymore!
                        // The GStreamer pipeline capsule string layout guarantees 960x540 RGBA bytes.
                        if let Some(buffer) = sample.buffer() {
                            if let Ok(map) = buffer.map_readable() {
                                if let Some(packed) = pool_ingest.try_claim() {
                                    pool_ingest.with_payload_mut(packed, |payload| {
                                        payload.copy_from_slice(map.as_slice());
                                    });

                                    while let Err(_) = video_ingested_prod.push(packed) {
                                        std::thread::sleep(std::time::Duration::from_micros(100));
                                    }
                                } else {
                                    std::thread::sleep(std::time::Duration::from_micros(100));
                                }
                            }
                        }
                    }
                    Err(_) => {
                        println!("[INGEST] Pipeline reached End of Stream or crashed.");
                        break;
                    }
                }
            }

            println!("[INGEST] Exiting ingest thread.");
            pipeline.set_state(gstreamer::State::Null).unwrap();
        });
        self.ingest_handle = Some(ingest_handle);

        // --- ML worker thread loop ---
        let pool_ml = pool.clone();
        let video_ingested_cons = video_ingested.clone();
        let video_ml_ready_prod = video_ml_ready.clone();
        let model_ml = model.clone();
        let running_ml = running.clone();

        let ml_handle = std::thread::spawn(move || {
            while running_ml.load(std::sync::atomic::Ordering::Acquire) {
                if let Some(packed) = video_ingested_cons.pop() {
                    println!("[ML] Processing frame...");
                    let result = pool_ml.with_payload_mut(packed, |payload| {
                        model_ml.filter_frame(payload, WIDTH, HEIGHT)
                    });
                    if let Err(e) = result {
                        eprintln!("ML inference failed: {}", e);
                    }
                    pool_ml
                        .transition_state(packed, STATE_INGESTED, STATE_ML_COMMITTED)
                        .expect("State transition failed");
                    video_ml_ready_prod.push(packed).unwrap();
                } else {
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
            }
        });
        self.ml_handle = Some(ml_handle);

        // Upload
        let pool_upload = pool.clone();
        let video_ml_ready_cons = video_ml_ready.clone();
        let video_gpu_upload_ready_prod = video_gpu_upload_ready.clone();
        let device_upload = self.device.as_ref().unwrap().clone();
        let queue_upload = self.queue.as_ref().unwrap().clone();
        let texture_upload = self.texture.as_ref().unwrap().clone();
        let running_upload = running.clone();
        let upload_handle = std::thread::spawn(move || {
            while running_upload.load(std::sync::atomic::Ordering::Acquire) {
                if let Some(packed) = video_ml_ready_cons.pop() {
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
                        .transition_state(packed, STATE_ML_COMMITTED, STATE_GPU_UPLOADED)
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
                let surface = self.surface.as_ref().unwrap();
                let device = self.device.as_ref().unwrap();
                let queue = self.queue.as_ref().unwrap();
                let render_pipeline = self.render_pipeline.as_ref().unwrap();
                let vertex_buffer = self.vertex_buffer.as_ref().unwrap();
                let bind_group = self.bind_group.as_ref().unwrap();
                let pool = self.pool.as_ref().unwrap();
                let video_gpu_upload_ready = self.video_gpu_upload_ready.as_ref().unwrap();

                // Pop any ready frame and release it (already uploaded).
                if let Some(packed) = video_gpu_upload_ready.pop() {
                    pool.release_video(packed);
                }

                // Wait until we have at least 30 frames ready to avoid startup glitches.
                // This ensures the render loop doesn't start too early while the buffer is empty.
                let ready_count = video_gpu_upload_ready.len();
                println!("[RENDER] Buffer count: {}", ready_count);
                if ready_count < 1 {
                    // Not enough frames yet – request another redraw and skip this frame.
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
                            // Ensure GPU is idle before reconfiguring
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
                        let size = self.window.as_ref().unwrap().inner_size();
                        if size.width > 0 && size.height > 0 {
                            let config = self.config.as_mut().unwrap();
                            config.width = size.width;
                            config.height = size.height;
                            // Ensure GPU is idle before reconfiguring
                            device
                                .poll(wgpu::PollType::Wait {
                                    submission_index: None,
                                    timeout: None,
                                })
                                .expect("Failed to poll GPU before reconfiguring (Suboptimal)");
                            surface.configure(device, config);
                        }
                    }
                    wgpu::CurrentSurfaceTexture::Lost => {
                        eprintln!("Surface lost");
                    }
                    wgpu::CurrentSurfaceTexture::Validation => {
                        eprintln!("Validation error");
                    }
                }
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
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
