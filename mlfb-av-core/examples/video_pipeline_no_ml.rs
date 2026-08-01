use crossbeam::queue::ArrayQueue;
use image::RgbaImage;
use mlfb_av_core::memory::{PackedIndex, STATE_GPU_UPLOADED, STATE_INGESTED, SlotPool};
use std::cmp::min;
use std::sync::Arc;
use std::sync::Once;
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
use yscv_video::Mp4VideoReader;

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
const VIDEO_SLOT_SIZE: usize = (WIDTH * HEIGHT * 4) as usize;
const N_V: usize = 128;
static SAVE_FRAME: Once = Once::new();

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
    video_reader: Option<Mp4VideoReader>,

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
            video_reader: None,
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

        // --- Video Pipeline ---
        let pool = Arc::new(SlotPool::<VIDEO_SLOT_SIZE>::new(N_V));
        self.pool = Some(pool.clone());

        let video_ingested = Arc::new(ArrayQueue::<PackedIndex>::new(N_V));
        let video_gpu_upload_ready = Arc::new(ArrayQueue::<PackedIndex>::new(N_V));
        self.video_ingested = Some(video_ingested.clone());
        self.video_gpu_upload_ready = Some(video_gpu_upload_ready.clone());

        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

        // --- Open video reader on main thread ---
        let reader = match Mp4VideoReader::open("assets/video.mp4".as_ref()) {
            Ok(r) => {
                println!("[MAIN] Video opened successfully.");
                r
            }
            Err(e) => {
                eprintln!("[MAIN] Failed to open video: {}", e);
                return;
            }
        };
        self.video_reader = Some(reader);

        // --- Upload thread (consumes from video_ingested directly) ---
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
                    println!(
                        "[UPLOAD] First 16 bytes of payload: {:02x?}",
                        &payload[..min(16, payload.len())]
                    );
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
                // --- Main-thread video ingest ---
                if let Some(reader) = self.video_reader.as_mut() {
                    match reader.next_frame() {
                        Ok(Some(frame)) => {
                            // Convert RGB -> RGBA
                            let rgb = &frame.rgb8_data;
                            let width = frame.width as usize;
                            let height = frame.height as usize;
                            let mut rgba = Vec::with_capacity(width * height * 4);
                            for chunk in rgb.chunks_exact(3) {
                                rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 255]);
                            }

                            // Resize to target dimensions
                            let src_img = fast_image_resize::images::Image::from_slice_u8(
                                width as u32,
                                height as u32,
                                &mut rgba,
                                fast_image_resize::PixelType::U8x4,
                            )
                            .unwrap();
                            let mut dst_img = fast_image_resize::images::Image::new(
                                WIDTH,
                                HEIGHT,
                                fast_image_resize::PixelType::U8x4,
                            );
                            let mut resizer = fast_image_resize::Resizer::new();
                            let options = fast_image_resize::ResizeOptions::new().resize_alg(
                                fast_image_resize::ResizeAlg::Convolution(
                                    fast_image_resize::FilterType::Bilinear,
                                ),
                            );
                            resizer
                                .resize(&src_img, &mut dst_img, Some(&options))
                                .unwrap();
                            let resized_data = dst_img.buffer().to_vec();
                            SAVE_FRAME.call_once(|| {
                                // Use RgbaImage to infer pixel type
                                if let Some(img) =
                                    RgbaImage::from_raw(WIDTH, HEIGHT, resized_data.clone())
                                {
                                    if let Err(e) = img.save("first_frame.png") {
                                        eprintln!("Failed to save frame: {}", e);
                                    } else {
                                        println!("Saved first_frame.png");
                                    }
                                } else {
                                    eprintln!("Failed to create RgbaImage from raw data");
                                }
                            });
                            println!(
                                "[MAIN] First 16 bytes of resized data: {:02x?}",
                                &resized_data[..min(16, resized_data.len())]
                            );

                            // Push to the queue
                            if let Some(pool) = self.pool.as_ref() {
                                if let Some(packed) = pool.try_claim() {
                                    pool.with_payload_mut(packed, |payload| {
                                        payload.copy_from_slice(&resized_data);
                                    });
                                    if let Some(queue) = self.video_ingested.as_ref() {
                                        while let Err(_) = queue.push(packed) {
                                            std::thread::sleep(std::time::Duration::from_micros(
                                                100,
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        Ok(None) => {
                            println!("[MAIN] End of video stream");
                            self.video_reader = None;
                        }
                        Err(e) => {
                            eprintln!("[MAIN] Decode error: {}", e);
                            self.video_reader = None;
                        }
                    }
                }

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

                let ready_count = video_gpu_upload_ready.len();
                println!("[RENDER] Buffer count: {}", ready_count);
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
                                            r: 1.0,
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

                // Keep redrawing if video is still playing
                if self.video_reader.is_some() {
                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
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
