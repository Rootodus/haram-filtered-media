use gst::glib::ControlFlow;
use gstreamer as gst;
use gstreamer::glib::object::Cast;
use gstreamer::prelude::GstBinExtManual;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use gst::{ClockTime, SeekFlags};
use hfm_core::ml::PeopleSegFilter;
use hfm_core::pipeline::{FrameSource, HEIGHT, VideoPipeline, WIDTH};
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

// ---- GStreamer source implementation ----
struct GstSource {
    pipeline: gst::Pipeline,
    sink: gst_app::AppSink,
    _seekable: bool,
}

impl GstSource {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        gst::init()?;

        let pipeline = gst::Pipeline::new();
        let video_path = format!("{}/assets/video.mp4", env!("CARGO_MANIFEST_DIR"));
        let src = gst::ElementFactory::make("filesrc")
            .property("location", video_path)
            .build()?;
        let decodebin = gst::ElementFactory::make("decodebin").build()?;
        let convert = gst::ElementFactory::make("videoconvert").build()?;
        let scale = gst::ElementFactory::make("videoscale").build()?;
        let sink = gst_app::AppSink::builder()
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

        pipeline.add_many(&[&src, &decodebin, &convert, &scale, &sink_element])?;
        gst::Element::link_many(&[&src, &decodebin])?;
        gst::Element::link_many(&[&convert, &scale, &sink_element])?;

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
                    eprintln!("Failed to link decodebin pad to convert: {}", e);
                }
            }
        });

        pipeline.set_state(gst::State::Playing)?;

        // Watch bus for errors
        let bus = pipeline.bus().expect("No bus");
        let _guard = bus.add_watch(move |_, msg| {
            use gst::MessageView;
            match msg.view() {
                MessageView::Error(err) => {
                    eprintln!("GStreamer error: {}", err.error());
                    if let Some(debug) = err.debug() {
                        eprintln!("Debug info: {}", debug);
                    }
                    ControlFlow::Break
                }
                MessageView::Eos(_) => {
                    println!("End of stream");
                    ControlFlow::Break
                }
                _ => ControlFlow::Continue,
            }
        })?;

        Ok(Self {
            pipeline,
            sink,
            _seekable: true,
        })
    }
}

impl FrameSource for GstSource {
    fn pull_frame(&mut self) -> Option<(Vec<u8>, u64)> {
        let sample = match self.sink.pull_sample() {
            Ok(s) => s,
            Err(_) => return None,
        };
        let buffer = sample.buffer()?;
        let map = buffer.map_readable().ok()?;
        let data = map.as_slice().to_vec();
        let pts_ns = buffer.pts().map(|c| c.nseconds()).unwrap_or(0);
        Some((data, pts_ns))
    }

    fn seek(&mut self, delta_ns: i64) -> Result<(), String> {
        let current_pos = self
            .pipeline
            .query_position::<ClockTime>()
            .unwrap_or_else(|| ClockTime::from_seconds(0));
        let current_ns = current_pos.nseconds() as i64;
        let new_ns = (current_ns + delta_ns).max(0);
        let new_pos = ClockTime::from_nseconds(new_ns as u64);
        self.pipeline
            .seek_simple(SeekFlags::FLUSH, new_pos)
            .map_err(|e| format!("Seek failed: {:?}", e))
    }
}

// ---- WGPU renderer ----
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
    pipeline: Option<VideoPipeline>,
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
            pipeline: None,
        }
    }
}

impl App {
    fn init_gpu(&mut self, event_loop: &ActiveEventLoop) -> (Instance, wgpu::Adapter) {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("Video Pipeline")
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

        (instance, adapter)
    }

    fn configure_surface(&mut self, adapter: &wgpu::Adapter) {
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

        let caps = self.surface.as_ref().unwrap().get_capabilities(adapter);
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
    }

    fn init_texture_and_sampler(&mut self) {
        let device = self.device.as_ref().unwrap();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
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
    }

    fn init_vertex_buffer(&mut self) {
        let device = self.device.as_ref().unwrap();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(&QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });
        self.vertex_buffer = Some(vertex_buffer);
    }

    fn init_bind_group(&mut self) {
        let device = self.device.as_ref().unwrap();
        let texture_view = self
            .texture
            .as_ref()
            .unwrap()
            .create_view(&wgpu::TextureViewDescriptor::default());

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Video Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(self.sampler.as_ref().unwrap()),
                },
            ],
        });
        self.bind_group = Some(bind_group);
    }

    fn init_render_pipeline(&mut self) {
        let device = self.device.as_ref().unwrap();
        let format = self.config.as_ref().unwrap().format;

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Texture Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/texture_quad.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
    }

    fn init_pipeline(&mut self) {
        let source = GstSource::new().expect("Failed to create GStreamer source");
        let model = PeopleSegFilter::new("models/pphumanseg.onnx").expect("Failed to load model");
        let mut pipeline = VideoPipeline::new(Box::new(source), model);
        pipeline.start();
        self.pipeline = Some(pipeline);
    }

    fn render_frame(&mut self) {
        let surface = self.surface.as_ref().unwrap();
        let device = self.device.as_ref().unwrap();
        let queue = self.queue.as_ref().unwrap();
        let render_pipeline = self.render_pipeline.as_ref().unwrap();
        let vertex_buffer = self.vertex_buffer.as_ref().unwrap();
        let bind_group = self.bind_group.as_ref().unwrap();

        // Try to get a processed frame
        if let Some(frame) = self.pipeline.as_ref().unwrap().pop_processed_frame() {
            // Upload to GPU
            let payload = frame.data;
            let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Frame Staging"),
                contents: &payload,
                usage: wgpu::BufferUsages::COPY_SRC,
            });
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                    texture: self.texture.as_ref().unwrap(),
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
            queue.submit(Some(encoder.finish()));
        }

        // Render the quad
        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => {
                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
            _ => {}
        }

        self.window.as_ref().unwrap().request_redraw();
    }

    fn _reconfigure_surface(&mut self) {
        let size = self.window.as_ref().unwrap().inner_size();
        if size.width > 0 && size.height > 0 {
            let config = self.config.as_mut().unwrap();
            config.width = size.width;
            config.height = size.height;
            let device = self.device.as_ref().unwrap();
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .expect("Failed to poll GPU before reconfiguring");
            self.surface.as_ref().unwrap().configure(device, config);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let (_, adapter) = self.init_gpu(event_loop);
        self.configure_surface(&adapter);
        self.init_texture_and_sampler();
        self.init_vertex_buffer();
        self.init_bind_group();
        self.init_render_pipeline();
        self.init_pipeline();

        self.window.as_ref().unwrap().request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => self.render_frame(),
            WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        logical_key: winit::keyboard::Key::Named(named_key),
                        state: winit::event::ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                const SEEK_DELTA_NS: i64 = 10_000_000_000;
                if let Some(pipeline) = self.pipeline.as_ref() {
                    match named_key {
                        winit::keyboard::NamedKey::ArrowLeft => {
                            let _ = pipeline.seek(-SEEK_DELTA_NS);
                        }
                        winit::keyboard::NamedKey::ArrowRight => {
                            let _ = pipeline.seek(SEEK_DELTA_NS);
                        }
                        _ => {}
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
