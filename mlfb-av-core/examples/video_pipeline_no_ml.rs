use crossbeam::queue::ArrayQueue;
use mlfb_av_core::memory::{PackedIndex, STATE_GPU_UPLOADED, STATE_INGESTED, SlotPool};
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

// ---- Symphonia + rav1d imports ----
use symphonia::core::formats::FormatOptions;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::default::get_probe;

use rav1d::include::dav1d::data::Dav1dData;
use rav1d::include::dav1d::dav1d::{Dav1dContext, Dav1dSettings};
use rav1d::include::dav1d::picture::Dav1dPicture;
use rav1d::src::lib::{
    dav1d_close, dav1d_data_create, dav1d_data_unref, dav1d_default_settings, dav1d_flush,
    dav1d_get_picture, dav1d_open, dav1d_picture_unref, dav1d_send_data,
};
use std::mem::MaybeUninit;
use std::ptr::NonNull;

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

        // ---- Ingest thread (video decoder) ----
        let pool_ingest = pool.clone();
        let video_ingested_prod = video_ingested.clone();
        let running_ingest = running.clone();

        let ingest_handle = std::thread::spawn(move || {
            // ---- Open video with Symphonia ----
            let file_path = "assets/video.mp4";
            let source = std::fs::File::open(file_path).expect("Failed to open video file");
            let mss = MediaSourceStream::new(Box::new(source), Default::default());
            let hint = Hint::new();
            let format_opts = FormatOptions::default();
            let meta_opts = MetadataOptions::default();

            let mut format_reader = get_probe()
                .probe(&hint, mss, format_opts, meta_opts)
                .expect("Failed to probe file");

            let video_track = format_reader
                .tracks()
                .iter()
                .find(|track| {
                    track
                        .codec_params
                        .as_ref()
                        .map_or(false, |cp| cp.is_video())
                })
                .cloned()
                .expect("No video track");

            println!("[INGEST] Using video track id: {}", video_track.id);

            // ---- Init rav1d ----
            let mut settings = MaybeUninit::<Dav1dSettings>::uninit();
            unsafe {
                dav1d_default_settings(NonNull::new(settings.as_mut_ptr()).unwrap());
            }
            let settings = unsafe { settings.assume_init() };

            let mut ctx_ptr: Option<Dav1dContext> = None;
            let res = unsafe {
                dav1d_open(
                    Some(NonNull::from(&mut ctx_ptr)),
                    Some(NonNull::from(&settings)),
                )
            };
            if res.0 != 0 {
                panic!("dav1d_open failed: {}", res.0);
            }
            let ctx = ctx_ptr.unwrap();

            let mut obu_buffer = Vec::new();
            let target_w = WIDTH as usize;
            let target_h = HEIGHT as usize;
            let mut frame_count = 0;

            while running_ingest.load(std::sync::atomic::Ordering::Acquire) {
                match format_reader.next_packet() {
                    Ok(Some(packet)) => {
                        if packet.track_id != video_track.id {
                            continue;
                        }
                        // Accumulate raw packet data (it's already OBU data)
                        obu_buffer.extend_from_slice(&packet.data);

                        if !obu_buffer.is_empty() {
                            // Allocate internal buffer
                            let mut dav1d_data = Dav1dData::default();
                            let buf_ptr = unsafe {
                                dav1d_data_create(
                                    Some(NonNull::from(&mut dav1d_data)),
                                    obu_buffer.len(),
                                )
                            };
                            if buf_ptr.is_null() {
                                eprintln!("[INGEST] dav1d_data_create failed");
                                obu_buffer.clear();
                                continue;
                            }
                            unsafe {
                                std::ptr::copy_nonoverlapping(
                                    obu_buffer.as_ptr(),
                                    buf_ptr,
                                    obu_buffer.len(),
                                );
                            }
                            let res = unsafe {
                                dav1d_send_data(Some(ctx), Some(NonNull::from(&mut dav1d_data)))
                            };
                            if res.0 != 0 {
                                unsafe { dav1d_data_unref(Some(NonNull::from(&mut dav1d_data))) }
                                obu_buffer.clear();
                                if res.0 != -11 {
                                    eprintln!("[INGEST] send_data error: {}", res.0);
                                }
                                continue;
                            }
                            obu_buffer.clear();

                            // Try to retrieve decoded frames
                            loop {
                                let mut picture = MaybeUninit::<Dav1dPicture>::uninit();
                                let res = unsafe {
                                    dav1d_get_picture(
                                        Some(ctx),
                                        Some(NonNull::new(picture.as_mut_ptr()).unwrap()),
                                    )
                                };
                                if res.0 == 0 {
                                    let mut picture = unsafe { picture.assume_init() };
                                    frame_count += 1;
                                    if frame_count % 10 == 0 {
                                        println!("[INGEST] Decoded frame #{}", frame_count);
                                    }

                                    let w = picture.p.w as usize;
                                    let h = picture.p.h as usize;
                                    let y_ptr = picture.data[0].unwrap().as_ptr() as *const u8;
                                    let u_ptr = picture.data[1].unwrap().as_ptr() as *const u8;
                                    let v_ptr = picture.data[2].unwrap().as_ptr() as *const u8;
                                    let y_stride = picture.stride[0] as usize;
                                    let uv_stride = picture.stride[1] as usize;

                                    // Convert YUV to RGBA
                                    let mut rgba = vec![0u8; w * h * 4];
                                    for row in 0..h {
                                        for col in 0..w {
                                            let y_idx = row * y_stride + col;
                                            let uv_idx = (row / 2) * uv_stride + (col / 2);
                                            unsafe {
                                                let y_val = *y_ptr.add(y_idx) as f32;
                                                let u_val = *u_ptr.add(uv_idx) as f32 - 128.0;
                                                let v_val = *v_ptr.add(uv_idx) as f32 - 128.0;
                                                let r = (y_val + 1.5748 * v_val).clamp(0.0, 255.0)
                                                    as u8;
                                                let g = (y_val - 0.1873 * u_val - 0.4681 * v_val)
                                                    .clamp(0.0, 255.0)
                                                    as u8;
                                                let b = (y_val + 1.8556 * u_val).clamp(0.0, 255.0)
                                                    as u8;
                                                let idx = (row * w + col) * 4;
                                                rgba[idx] = r;
                                                rgba[idx + 1] = g;
                                                rgba[idx + 2] = b;
                                                rgba[idx + 3] = 255;
                                            }
                                        }
                                    }

                                    // Resize to target dimensions
                                    let src_img = fast_image_resize::images::Image::from_slice_u8(
                                        w as u32,
                                        h as u32,
                                        &mut rgba,
                                        fast_image_resize::PixelType::U8x4,
                                    )
                                    .unwrap();
                                    let mut dst_img = fast_image_resize::images::Image::new(
                                        target_w as u32,
                                        target_h as u32,
                                        fast_image_resize::PixelType::U8x4,
                                    );
                                    let mut resizer = fast_image_resize::Resizer::new();
                                    let options = fast_image_resize::ResizeOptions::new()
                                        .resize_alg(fast_image_resize::ResizeAlg::Convolution(
                                            fast_image_resize::FilterType::Bilinear,
                                        ));
                                    resizer
                                        .resize(&src_img, &mut dst_img, Some(&options))
                                        .unwrap();
                                    let resized_data = dst_img.buffer().to_vec();

                                    // Claim a slot and push to queue
                                    if let Some(packed) = pool_ingest.try_claim() {
                                        pool_ingest.with_payload_mut(packed, |payload| {
                                            payload.copy_from_slice(&resized_data);
                                        });
                                        while let Err(_) = video_ingested_prod.push(packed) {
                                            std::thread::sleep(std::time::Duration::from_micros(
                                                100,
                                            ));
                                        }
                                    } else {
                                        // No free slot; drop frame
                                        std::thread::sleep(std::time::Duration::from_micros(100));
                                    }

                                    unsafe {
                                        dav1d_picture_unref(Some(NonNull::from(&mut picture)))
                                    }
                                } else if res.0 == -11 {
                                    break; // EAGAIN, no more frames ready
                                } else {
                                    eprintln!("[INGEST] get_picture error: {}", res.0);
                                    break;
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        // End of stream – flush decoder
                        println!("[INGEST] End of stream, flushing decoder");
                        unsafe { dav1d_flush(ctx) };
                        // Try to get any remaining frames
                        loop {
                            let mut picture = MaybeUninit::<Dav1dPicture>::uninit();
                            let res = unsafe {
                                dav1d_get_picture(
                                    Some(ctx),
                                    Some(NonNull::new(picture.as_mut_ptr()).unwrap()),
                                )
                            };
                            if res.0 == 0 {
                                let mut picture = unsafe { picture.assume_init() };
                                // Process flush frame (simplified: just unref)
                                unsafe { dav1d_picture_unref(Some(NonNull::from(&mut picture))) }
                            } else {
                                break;
                            }
                        }
                        break;
                    }
                    Err(e) => {
                        eprintln!("[INGEST] Demux error: {}", e);
                        break;
                    }
                }
            }

            // Cleanup
            unsafe { dav1d_flush(ctx) };
            let mut ctx_ptr2 = Some(ctx);
            unsafe { dav1d_close(Some(NonNull::from(&mut ctx_ptr2))) };
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
                    println!("[UPLOAD] Popped frame from ingested queue");
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
                    println!("[UPLOAD] Submitted frame to GPU");
                    video_gpu_upload_ready_prod.push(packed).unwrap();
                    println!(
                        "[UPLOAD] Pushed to gpu_upload_ready, queue len: {}",
                        video_gpu_upload_ready_prod.len()
                    );
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
                        // Similar to Success, but also reconfigure
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
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
