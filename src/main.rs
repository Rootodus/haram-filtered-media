use std::error::Error;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

mod protocol;
use protocol::Metadata;

const ADDR: &str = "127.0.0.1:8080";

struct SharedState {
    latest_frame: Option<Vec<u8>>,
    meta: Option<Metadata>,
    dirty: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("MLFB Native Runtime - Spike 03")
            .with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0)) // Scaled down for laptop testing
            .build(&event_loop)?,
    );

    let shared_state = Arc::new(Mutex::new(SharedState {
        latest_frame: None,
        meta: None,
        dirty: false,
    }));

    // Spawn IPC Listener
    let state_for_ipc = shared_state.clone();
    tokio::spawn(async move {
        let listener = TcpListener::bind(ADDR).await.unwrap();
        println!("IPC Listener active on {}", ADDR);
        while let Ok((mut stream, _)) = listener.accept().await {
            let _ = stream.set_nodelay(true);
            let _ = handle_connection(&mut stream, state_for_ipc.clone()).await;
        }
    });

    // WGPU 0.19 Initialization
    let instance = wgpu::Instance::default();
    let surface = unsafe { instance.create_surface(window.as_ref()) }?;
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .ok_or("Failed to find adapter")?;

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await?;

    let caps = surface.get_capabilities(&adapter);
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_DST,
        format: caps.formats[0],
        width: 1280,
        height: 720,
        present_mode: wgpu::PresentMode::Fifo, // Sync to monitor
        alpha_mode: caps.alpha_modes[0],
        view_formats: vec![],
        desired_maximum_frame_latency: 2,
    };
    surface.configure(&device, &config);

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => {
                let mut state = shared_state.lock().unwrap();
                if state.dirty {
                    if let (Some(_pixels), Some(meta)) = (&state.latest_frame, &state.meta) {
                        // Spike verification: Only print on frame arrival
                        println!(
                            "Processing frame: {}x{} [Simulated GPU Upload]",
                            meta.width, meta.height
                        );
                        state.dirty = false;
                    }
                }

                let output = match surface.get_current_texture() {
                    Ok(f) => f,
                    Err(_) => return,
                };
                let view = output
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder =
                    device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                {
                    let _rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: None,
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLUE), // Blue = Runtime Active
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    });
                }

                queue.submit(Some(encoder.finish()));
                output.present();
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => elwt.exit(),
            Event::AboutToWait => window.request_redraw(),
            _ => {}
        }
    })?;

    Ok(())
}

async fn handle_connection(
    stream: &mut TcpStream,
    state: Arc<Mutex<SharedState>>,
) -> Result<(), Box<dyn Error>> {
    let mut len_buf = [0u8; 4];
    let mut meta_payload = Vec::with_capacity(1024);
    let mut pixel_payload = Vec::new();

    loop {
        if stream.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let meta_len = u32::from_le_bytes(len_buf) as usize;

        unsafe {
            meta_payload.set_len(meta_len);
        }
        stream.read_exact(&mut meta_payload).await?;
        let meta: protocol::Metadata = rmp_serde::from_slice(&meta_payload)?;

        let pixel_bytes = (meta.width * meta.height * 4) as usize;
        pixel_payload.resize(pixel_bytes, 0);

        stream.read_exact(&mut pixel_payload).await?;

        {
            let mut lock = state.lock().unwrap();
            lock.latest_frame = Some(pixel_payload.clone());
            lock.meta = Some(meta);
            lock.dirty = true;
        }

        stream.write_u8(0x01).await?;
    }
    Ok(())
}
