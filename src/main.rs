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
    // Channel to wake up the IPC thread after a frame is displayed
    pub ack_sender: tokio::sync::mpsc::Sender<()>,
    // Persistent color to prevent "White Window" during idle
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

                let mut needs_ack = false;

                // --- PHASE 1: Data Update (Conditional) ---
                if let Some(frame) = self.state.get_frame_if_dirty() {
                    let texture_size = Extent3d {
                        width: frame.meta.width,
                        height: frame.meta.height,
                        depth_or_array_layers: 1,
                    };

                    if self.frame_texture.as_ref().map_or(true, |t| {
                        t.width() != frame.meta.width || t.height() != frame.meta.height
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
                            bytes_per_row: Some(4 * frame.meta.width),
                            rows_per_image: Some(frame.meta.height),
                        },
                        texture_size,
                    );

                    // Update persistent color based on pulse
                    if let Ok(mut color) = self.state.clear_color.lock() {
                        let pulse = (frame.meta.timestamp % 1000) as f64 / 1000.0;
                        *color = Color {
                            r: 0.1,
                            g: pulse,
                            b: 0.3,
                            a: 1.0,
                        };
                    }
                    needs_ack = true;
                }

                // --- PHASE 2: Render Pass (Every Frame) ---
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

                // --- PHASE 3: End-to-End Signaling ---
                if needs_ack {
                    // Wake up the IPC thread only after present()
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

async fn handle_connection(
    mut stream: TcpStream,
    state: Arc<SharedAppState>,
    mut ack_receiver: tokio::sync::mpsc::Receiver<()>,
) -> Result<(), Box<dyn Error>> {
    let _ = stream.set_nodelay(true);
    let mut len_buf = [0u8; 4];
    let mut meta_payload = Vec::with_capacity(1024);

    loop {
        if stream.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let meta_len = u32::from_le_bytes(len_buf) as usize;

        meta_payload.resize(meta_len, 0);
        stream.read_exact(&mut meta_payload).await?;
        let meta: Metadata = rmp_serde::from_slice(&meta_payload)?;

        // Capture properties before meta is moved
        let current_ts = meta.timestamp;
        let pixel_bytes = (meta.width * meta.height * 4) as usize;

        let mut pixel_vec = vec![0u8; pixel_bytes];

        let start_io = std::time::Instant::now();
        stream.read_exact(&mut pixel_vec).await?;
        let io_duration = start_io.elapsed();

        // 1. Hand off to renderer (Moves ownership of meta)
        state.update_frame(meta, pixel_vec);

        // 2. CRITICAL: Wait for the Renderer to signal that the frame hit the screen
        if ack_receiver.recv().await.is_none() {
            break;
        }

        // 3. Send ACK to JS Loader
        stream.write_all(&[0x01]).await?;

        // Use captured timestamp for logging
        if current_ts % 100 == 0 {
            println!("Net IO Duration: {:?}", io_duration);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Create the sync channel
    let (ack_tx, ack_rx) = tokio::sync::mpsc::channel::<()>(1);

    // Initialize state with the sender
    let state = Arc::new(SharedAppState::new(ack_tx));

    let state_for_ipc = state.clone();
    tokio::spawn(async move {
        let addr = "127.0.0.1:8080";
        let listener = TcpListener::bind(addr)
            .await
            .expect("Failed to bind TCP listener");
        println!("Listening on {}...", addr);

        // For this spike, we pass the single rx to the first connection
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
