use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use winit::event_loop::EventLoop;

// The generated types are in the `flatbuffers` module
use ml_filtered_browser::Metadata;

/// Shared frame data: FlatBuffer bytes + raw pixel data
#[derive(Clone)] // Required for `lock.clone()`
pub struct FrameState {
    pub timestamp: u64,
    pub width: u32,
    pub height: u32,
    pub buffer: Arc<[u8]>, // FlatBuffer bytes
    pub pixel_data: Arc<[u8]>,
}

/// High-performance shared state
pub struct SharedAppState {
    pub frame: Mutex<Option<FrameState>>,
    pub dirty: AtomicBool,
    pub ack_sender: tokio::sync::mpsc::Sender<()>,
    pub clear_color: Mutex<wgpu::Color>,
}

impl SharedAppState {
    pub fn new(ack_sender: tokio::sync::mpsc::Sender<()>) -> Self {
        Self {
            frame: Mutex::new(None),
            dirty: AtomicBool::new(false),
            ack_sender,
            clear_color: Mutex::new(wgpu::Color {
                r: 0.01,
                g: 0.01,
                b: 0.1,
                a: 1.0,
            }),
        }
    }

    pub fn update_frame(
        &self,
        timestamp: u64,
        width: u32,
        height: u32,
        buffer: Arc<[u8]>,
        pixel_data: Arc<[u8]>,
    ) {
        let new_frame = FrameState {
            timestamp,
            width,
            height,
            buffer,
            pixel_data,
        };
        if let Ok(mut lock) = self.frame.lock() {
            *lock = Some(new_frame);
            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn get_frame_if_dirty(&self) -> Option<FrameState> {
        if !self.dirty.load(Ordering::Relaxed) {
            return None;
        }
        let lock = self.frame.lock().ok()?;
        self.dirty.store(false, Ordering::Relaxed);
        lock.clone()
    }
}

// ---------- wgpu and winit rendering ----------
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
}

impl App {
    pub fn new(state: Arc<SharedAppState>) -> Self {
        Self {
            window: None,
            surface: None,
            device: None,
            queue: None,
            state,
            frame_texture: None,
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
                    mock_inference(&frame);

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

                // --- PHASE 2: Render Pass ---
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

// ---------- Network Handling ----------
async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<SharedAppState>,
    mut ack_receiver: tokio::sync::mpsc::Receiver<()>,
) -> Result<(), Box<dyn Error>> {
    let _ = stream.set_nodelay(true);
    let mut len_buf = [0u8; 4];

    loop {
        // Read FlatBuffer length prefix
        if stream.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let fb_len = u32::from_le_bytes(len_buf) as usize;
        let mut fb_bytes = vec![0u8; fb_len];
        if stream.read_exact(&mut fb_bytes).await.is_err() {
            break;
        }

        // --- FlatBuffers verification timing ---
        let verify_start = Instant::now();
        // SAFETY: The loader is trusted. FlatBuffer is built by the same system.
        let metadata = unsafe { flatbuffers::root_unchecked::<Metadata>(&fb_bytes) };
        let verify_dur = verify_start.elapsed();

        let timestamp = metadata.timestamp();
        let width = metadata.width();
        let height = metadata.height();

        // --- Node vector length access (O(1), no iteration) ---
        let node_len_start = Instant::now();
        let nodes_opt = metadata.nodes();
        let node_count = nodes_opt.map(|v| v.len()).unwrap_or(0);
        let node_len_dur = node_len_start.elapsed();

        // Read raw pixel data
        let pixel_bytes = (width * height * 4) as usize;
        let mut pixel_vec = vec![0u8; pixel_bytes];
        if stream.read_exact(&mut pixel_vec).await.is_err() {
            break;
        }

        // Convert to Arc<[u8]>
        let fb_arc = Arc::from(fb_bytes.into_boxed_slice());
        let pixel_arc = Arc::from(pixel_vec.into_boxed_slice());

        state.update_frame(timestamp, width, height, fb_arc, pixel_arc);

        static LOG_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let count = LOG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count == 0 || count % 100 == 0 {
            println!(
                "Rust: verify={:?}, node_count={}, node_len_access={:?}, fb_bytes={}",
                verify_dur, node_count, node_len_dur, fb_len
            );
        }

        // Wait for GPU ACK and reply
        if ack_receiver.recv().await.is_none() {
            break;
        }
        if stream.write_all(&[0x01]).await.is_err() {
            break;
        }
    }
    Ok(())
}

fn mock_inference(frame: &FrameState) {
    // Touch pixel data
    let mut sum: u64 = 0;
    for i in (0..frame.pixel_data.len()).step_by(100) {
        sum += frame.pixel_data[i] as u64;
    }
    std::hint::black_box(sum);
    // Simulated ML latency
    std::thread::sleep(std::time::Duration::from_millis(10));
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (ack_tx, ack_rx) = tokio::sync::mpsc::channel::<()>(1);
    let state = Arc::new(SharedAppState::new(ack_tx));

    let state_for_ipc = state.clone();
    tokio::spawn(async move {
        let addr = "127.0.0.1:8080";
        let listener = TcpListener::bind(addr)
            .await
            .expect("Failed to bind TCP listener");
        println!("Listening on {}...", addr);

        let mut rx_holder = Some(ack_rx);
        while let Ok((stream, _)) = listener.accept().await {
            if let Some(rx) = rx_holder.take() {
                let s_handle = state_for_ipc.clone();
                tokio::spawn(async move {
                    let _ = handle_connection(stream, s_handle, rx).await;
                });
            }
        }
    });

    let event_loop = EventLoop::new()?;
    let mut app = App::new(state);
    println!("Starting Window Event Loop...");
    event_loop.run_app(&mut app)?;
    Ok(())
}
