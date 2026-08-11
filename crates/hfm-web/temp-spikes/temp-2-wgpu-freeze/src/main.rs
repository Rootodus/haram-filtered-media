use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};
struct App {
    rx: mpsc::Receiver<()>,
    frame_count: u32,
    redraw_count: u32,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    texture: Option<wgpu::Texture>,
    texture_view: Option<wgpu::TextureView>,
    bind_group: Option<wgpu::BindGroup>,
    pipeline: Option<wgpu::RenderPipeline>,
    viewport_size: (u32, u32),
}
impl App {
    fn init_graphics(&mut self, _event_loop: &ActiveEventLoop) {
        // Clone the Arc wrapper pointer directly to pass a safe 'static handle
        let window = self.window.clone().unwrap();
        let mut instance_desc = wgpu::InstanceDescriptor::new_without_display_handle();
        instance_desc.backends = wgpu::Backends::DX12;
        instance_desc.flags = wgpu::InstanceFlags::empty();
        let instance = wgpu::Instance::new(instance_desc);

        // Fix: Clone the Arc window handle explicitly here to avoid moving ownership
        let surface = instance.create_surface(Arc::clone(&window)).unwrap();

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
            apply_limit_buckets: false,
        }))
        .expect("Failed to find adapter");

        // Fix 2: Removed trailing path argument (None) since request_device only accepts one descriptor
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("Spike Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            experimental_features: Default::default(),
            trace: wgpu::Trace::Off,
        }))
        .expect("Failed to create device");

        let size = window.inner_size();
        self.viewport_size = (size.width.max(1), size.height.max(1));
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
            color_space: wgpu::SurfaceColorSpace::Srgb,
        };
        surface.configure(&device, &config);

        // Create a 128x128 texture for testing
        let texture_size = wgpu::Extent3d {
            width: 128,
            height: 128,
            depth_or_array_layers: 1,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Test Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&Default::default());

        // Simple shader: fullscreen quad with texture sampling
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "shader.wgsl"
            ))),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Pipeline Layout"),
            // Fix 3: Wrap layouts configuration array inside Some() matching the updated API expectations
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        // Store resources
        self.surface = Some(surface);
        self.device = Some(device);
        self.queue = Some(queue);
        self.texture = Some(texture);
        self.texture_view = Some(texture_view);
        self.bind_group = Some(bind_group);
        self.pipeline = Some(pipeline);
    }

    fn reconfigure_surface(&self) {
        if let (Some(surface), Some(device)) = (&self.surface, &self.device) {
            let config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                width: self.viewport_size.0,
                height: self.viewport_size.1,
                present_mode: wgpu::PresentMode::Fifo,
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                view_formats: vec![],
                desired_maximum_frame_latency: 2,
                color_space: wgpu::SurfaceColorSpace::Srgb,
            };
            surface.configure(device, &config);
        }
    }
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window_attributes = Window::default_attributes()
                .with_title("wgpu Freeze Spike")
                .with_inner_size(PhysicalSize::new(800, 600));
            let window = event_loop.create_window(window_attributes).unwrap();

            // Wrap with Arc here so it satisfies all required 'static bounds downstream
            self.window = Some(Arc::new(window));
            self.init_graphics(event_loop);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => {
                self.redraw_count += 1;
                if self.redraw_count % 60 == 0 {
                    println!("[Render] Redraw #{}", self.redraw_count);
                }
                // Simulate texture upload each frame
                let (device, queue) = (self.device.as_ref().unwrap(), self.queue.as_ref().unwrap());
                let texture = self.texture.as_ref().unwrap();
                // Generate random pixel data with all 4 channels (RGBA) per pixel
                let data: Vec<u8> = (0..128 * 128 * 4).map(|_| rand::random::<u8>()).collect();

                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * 128),
                        rows_per_image: Some(128),
                    },
                    wgpu::Extent3d {
                        width: 128,
                        height: 128,
                        depth_or_array_layers: 1,
                    },
                );

                // Render frame
                let surface_texture = match self.surface.as_ref().unwrap().get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t) => t,
                    _ => return,
                };
                let view = surface_texture.texture.create_view(&Default::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Encoder"),
                });
                {
                    let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.1,
                                    g: 0.2,
                                    b: 0.3,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        ..Default::default()
                    });
                    render_pass.set_pipeline(self.pipeline.as_ref().unwrap());
                    render_pass.set_bind_group(0, self.bind_group.as_ref().unwrap(), &[]);
                    render_pass.draw(0..4, 0..1);
                }
                // Submit rendering commands to the device queue
                queue.submit(std::iter::once(encoder.finish()));

                // Consume the surface texture and present the frame directly to the display window
                queue.present(surface_texture);
            }
            WindowEvent::Resized(new_size) => {
                let w = new_size.width.max(1);
                let h = new_size.height.max(1);
                self.viewport_size = (w, h);
                self.reconfigure_surface();
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Ok(()) = self.rx.try_recv() {
            self.frame_count += 1;
            if self.frame_count % 30 == 0 {
                let start = Instant::now();
                println!("[Sim] Inference start (blocking)");
                thread::sleep(Duration::from_millis(23));
                let elapsed = start.elapsed();
                println!("[Sim] Inference done in {:?}", elapsed);
            }
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }
}
fn main() {
    let event_loop = EventLoop::new().unwrap();

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let tick_duration = Duration::from_millis(16);
        loop {
            thread::sleep(tick_duration);
            if tx.send(()).is_err() {
                break;
            }
        }
    });
    let mut app = App {
        rx,
        frame_count: 0,
        redraw_count: 0,
        window: None,
        surface: None,
        device: None,
        queue: None,
        texture: None,
        texture_view: None,
        bind_group: None,
        pipeline: None,
        viewport_size: (1, 1),
    };
    event_loop.run_app(&mut app).unwrap();
}
