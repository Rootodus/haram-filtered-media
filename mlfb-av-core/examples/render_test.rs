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

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;

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
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    winit::window::WindowAttributes::default()
                        .with_title("Render Test")
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

        let surface = instance.create_surface(window).unwrap(); // moves Arc into surface
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

        // --- Texture ---
        let texture = self
            .device
            .as_ref()
            .unwrap()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("Test Texture"),
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

        // --- Upload test pattern ---
        let test_data: Vec<u8> = (0..(WIDTH * HEIGHT) as usize)
            .flat_map(|i| {
                let x = i % WIDTH as usize;
                let y = i / WIDTH as usize;
                let r = (x as f32 / WIDTH as f32 * 255.0) as u8;
                let g = (y as f32 / HEIGHT as f32 * 255.0) as u8;
                let b = 128;
                let a = 255;
                vec![r, g, b, a]
            })
            .collect();

        let staging =
            self.device
                .as_ref()
                .unwrap()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Test Pattern"),
                    contents: &test_data,
                    usage: wgpu::BufferUsages::COPY_SRC,
                });

        let mut encoder =
            self.device
                .as_ref()
                .unwrap()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Test Pattern Copy"),
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
        self.queue.as_ref().unwrap().submit(Some(encoder.finish()));

        // --- Sampler ---
        let sampler = self
            .device
            .as_ref()
            .unwrap()
            .create_sampler(&wgpu::SamplerDescriptor {
                label: Some("Sampler"),
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
                    label: Some("Shader"),
                    source: wgpu::ShaderSource::Wgsl(
                        include_str!("shaders/texture_quad.wgsl").into(),
                    ),
                });

        // --- Bind Group Layout ---
        let bind_group_layout = self.device.as_ref().unwrap().create_bind_group_layout(
            &wgpu::BindGroupLayoutDescriptor {
                label: Some("Bind Group Layout"),
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
                    label: Some("Bind Group"),
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

        // --- Request first redraw ---
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
                                            r: 0.1,
                                            g: 0.1,
                                            b: 0.1,
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
                                            r: 0.1,
                                            g: 0.1,
                                            b: 0.1,
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

                // Keep redrawing
                self.window.as_ref().unwrap().request_redraw();
            }
            _ => {}
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = App::default();
    event_loop.run_app(&mut app).unwrap();
}
