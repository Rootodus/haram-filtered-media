use serde::{Deserialize, Serialize};
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use winit::event_loop::EventLoop;

/// The binary contract for MessagePack synchronization
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Metadata {
    pub timestamp: u64,
    pub width: u32,
    pub height: u32,
}

/// The internal representation used by MLProcessor and Renderer
pub struct ContentBuffer<'a> {
    pub meta: Metadata,
    pub pixel_data: &'a [u8],
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VisualAction {
    pub action_type: u8,
    pub rect: [f32; 4],
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessedBuffer {
    pub instructions: Vec<VisualAction>,
}

/// Represents a single frame.
/// Using Arc<\[u8\]> (a slice) instead of Arc<Vec<u8>> eliminates
/// the secondary pointer indirection to the Vec's heap header.
#[derive(Clone)]
pub struct FrameState {
    pub meta: Metadata,
    pub pixel_data: Arc<[u8]>,
}

/// High-performance shared state for the native runtime.
/// Designed to be wrapped in an Arc for thread-safe access.
pub struct SharedAppState {
    pub frame: Mutex<Option<FrameState>>,
    pub dirty: AtomicBool,
}

impl SharedAppState {
    /// Creates a new state instance.
    pub fn new() -> Self {
        Self {
            frame: Mutex::new(None),
            dirty: AtomicBool::new(false),
        }
    }

    /// Updates the frame data.
    /// Converts Vec<u8> to Box<[u8]> then to Arc<[u8]> to ensure the
    /// allocation is exactly the size of the data with no extra capacity overhead.
    pub fn update_frame(&self, meta: Metadata, data: Vec<u8>) {
        // Prepare the state before locking
        let new_frame = FrameState {
            meta,
            // Convert Vec to Boxed slice then Arc to achieve Arc<[u8]>
            pixel_data: Arc::from(data.into_boxed_slice()),
        };

        if let Ok(mut lock) = self.frame.lock() {
            *lock = Some(new_frame);
            // Store true to signal new data is available
            self.dirty.store(true, Ordering::Release);
        }
    }

    /// Optimized check for the renderer.
    /// Performs an atomic check before attempting to acquire the Mutex.
    pub fn get_frame_if_dirty(&self) -> Option<FrameState> {
        // Atomic hint: if false, we avoid the Mutex lock entirely
        if !self.dirty.load(Ordering::Relaxed) {
            return None;
        }

        // Lock and extract a clone of the Arc handle
        let lock = self.frame.lock().ok()?;

        // Reset the flag
        self.dirty.store(false, Ordering::Relaxed);

        // Return a clone of the FrameState (clones the Metadata and the Arc handle)
        lock.clone()
    }
}

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
    // Persistent GPU texture storage
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
            present_mode: wgpu::PresentMode::Fifo,
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

                // 1. High-performance check for new frames
                if let Some(frame) = self.state.get_frame_if_dirty() {
                    let texture_size = Extent3d {
                        width: frame.meta.width,
                        height: frame.meta.height,
                        depth_or_array_layers: 1,
                    };

                    // 2. Texture Management: Create or Re-create if dimensions changed
                    let needs_recreation = self.frame_texture.as_ref().map_or(true, |t| {
                        t.width() != frame.meta.width || t.height() != frame.meta.height
                    });

                    if needs_recreation {
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

                    // 3. The Upload: Direct move of raw pixel slice into VRAM
                    // No cloning occurs here; frame.pixel_data is a slice reference
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
                            bytes_per_row: Some(4 * frame.meta.width),
                            rows_per_image: Some(frame.meta.height),
                        },
                        texture_size,
                    );

                    // 4. The Render Pass (Proof of Life)
                    let surface_texture = match surface.get_current_texture() {
                        wgpu::CurrentSurfaceTexture::Success(t) => t,
                        _ => return, // Handle Timeout/Outdated/Lost/OOM by skipping frame
                    };

                    let view = surface_texture.texture.create_view(&Default::default());
                    let mut encoder =
                        device.create_command_encoder(&CommandEncoderDescriptor { label: None });

                    // Generate a "pulse" color from the timestamp to visually verify frame arrival
                    let pulse = (frame.meta.timestamp % 1000) as f64 / 1000.0;

                    {
                        let _render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                            label: Some("Clear Pass"),
                            color_attachments: &[Some(RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                ops: Operations {
                                    load: LoadOp::Clear(Color {
                                        r: 0.1,
                                        g: pulse,
                                        b: 0.3,
                                        a: 1.0,
                                    }),
                                    store: StoreOp::Store,
                                },
                                depth_slice: None,
                            })],
                            ..Default::default()
                        });
                    }

                    queue.submit(std::iter::once(encoder.finish()));
                    surface_texture.present();
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

async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<SharedAppState>,
) -> Result<(), Box<dyn Error>> {
    let _ = stream.set_nodelay(true);
    let mut len_buf = [0u8; 4];
    let mut meta_payload = Vec::with_capacity(1024);

    loop {
        // 1. Read Metadata Length
        if stream.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let meta_len = u32::from_le_bytes(len_buf) as usize;

        // 2. Read and Deserialize Metadata
        meta_payload.resize(meta_len, 0);
        stream.read_exact(&mut meta_payload).await?;
        let meta: Metadata = rmp_serde::from_slice(&meta_payload)?;

        // 3. Read Raw Pixels directly into a new Vec
        let pixel_bytes = (meta.width * meta.height * 4) as usize;
        let mut pixel_vec = vec![0u8; pixel_bytes];

        let start_io = std::time::Instant::now();
        stream.read_exact(&mut pixel_vec).await?;
        let io_duration = start_io.elapsed();

        // 4. Zero-Copy Hand-off to Shared State
        state.update_frame(meta, pixel_vec);

        // 5. Backpressure ACK
        // Sending 0x01 signals the Loader that the Runtime is ready for the next frame
        stream.write_all(&[0x01]).await?;

        // Log locally to verify the pipe speed
        // println!("Pipe Latency: {:?}", io_duration);
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 1. Initialize Shared State
    let state = Arc::new(SharedAppState::new());

    // 2. Spawn IPC Listener Task (Producer Thread)
    let state_for_ipc = state.clone();
    tokio::spawn(async move {
        let addr = "127.0.0.1:8080";
        let listener = TcpListener::bind(addr)
            .await
            .expect("Failed to bind TCP listener");
        println!("Listening on {} [Optimized Pipeline Active]...", addr);

        while let Ok((stream, _)) = listener.accept().await {
            let state_handle = state_for_ipc.clone();
            tokio::spawn(async move {
                if let Err(e) = handle_connection(stream, state_handle).await {
                    eprintln!("IPC Connection Error: {}", e);
                }
            });
        }
    });

    // 3. Initialize and Run Graphics App (Consumer Thread / Main Thread)
    let event_loop = EventLoop::new()?;
    let mut app = App::new(state);

    println!("Starting Window Event Loop...");
    event_loop.run_app(&mut app)?;

    Ok(())
}
