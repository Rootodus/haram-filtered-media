use super::context::{ActionInstance, App, FinalUniforms, MaskUniforms};
use crate::types::{MAX_ACTIONS, VisualAction};

use bytemuck;
use std::sync::Once;
use std::sync::atomic::Ordering;
use wgpu::{
    Color, CommandEncoder, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, TextureFormat,
};

// ============================================================================
// SELF-CONTAINED RENDERDOC HOTKEY AND LIFE-CYCLE CONTROLLER
// ============================================================================

/// Tracks whether RenderDoc is actively recording a frame.
static IS_RECORDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Thread-safe type wrapper to hold the RenderDoc instance inside a global static OnceLock.
/// Wrapping it in a Mutex provides interior mutability and satisfies the Sync constraint safely.
struct RenderDocCell(pub std::sync::Mutex<renderdoc::RenderDoc<renderdoc::V141>>);
unsafe impl Send for RenderDocCell {}
unsafe impl Sync for RenderDocCell {}

/// Checks if a programmatic frame capture should be initiated.
pub fn manage_renderdoc_capture() {
    let debug = crate::debug_config::DebugConfig::get();
    if !debug.renderdoc_capture {
        return;
    }

    // Global safe cell container
    static RD_INSTANCE: std::sync::OnceLock<Option<RenderDocCell>> = std::sync::OnceLock::new();
    let rd = RD_INSTANCE.get_or_init(|| {
        renderdoc::RenderDoc::new()
            .ok()
            .map(|api| RenderDocCell(std::sync::Mutex::new(api)))
    });

    if let Some(RenderDocCell(mutex)) = rd {
        if !IS_RECORDING.load(Ordering::Acquire) {
            // Lock the mutex to get safe mutable access to the RenderDoc API handle
            if let Ok(mut rd_api) = mutex.lock() {
                if rd_api.is_target_control_connected() {
                    return;
                }

                println!("[RenderDoc] Triggering programmatic capture frame block start...");
                rd_api.start_frame_capture(std::ptr::null(), std::ptr::null());
                IS_RECORDING.store(true, Ordering::Release);
            }
        }
    }
}

/// Closes out the frame session trace and launches the replay view immediately.
pub fn finalize_renderdoc_capture() {
    static LAUNCH_UI: Once = Once::new();

    if IS_RECORDING.swap(false, Ordering::AcqRel) {
        if let Ok(mut rd_api) = renderdoc::RenderDoc::<renderdoc::V141>::new() {
            rd_api.end_frame_capture(std::ptr::null(), std::ptr::null());
            println!("[RenderDoc] Frame trace written successfully!");

            // Launch the replay UI only once, even if called multiple times
            LAUNCH_UI.call_once(|| {
                let _ = rd_api.launch_replay_ui(true, None);
            });
        }
    }
}

/// Saves the compiled composited screen frame directly to a data file on disk.
pub fn debug_dump_frame_headless(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    render_pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    uniform_buffer: &wgpu::Buffer,
    screenshot_size: (u32, u32),
    scratch_pad: &mut Vec<u8>,
) {
    let width = screenshot_size.0;
    let height = screenshot_size.1;

    // 1. Create a headless texture target matching your screen dimensions
    let headless_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Headless Capture Target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let headless_view = headless_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // 2. Build an alignment buffer to read the GPU data back to the CPU
    let bytes_per_pixel = 4;
    let unpadded_bytes_per_row = width * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padding = (align - unpadded_bytes_per_row % align) % align;
    let padded_bytes_per_row = unpadded_bytes_per_row + padding;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Headless Output Buffer"),
        size: (padded_bytes_per_row * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Headless Capture Encoder"),
    });

    // 3. Execute the compositing passes inside our headless render target view
    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Headless Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &headless_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        render_pass.set_pipeline(render_pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }

    // 4. Copy the rendered pixels into our output alignment buffer
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &headless_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &output_buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(std::iter::once(encoder.finish()));

    // 5. Map the buffer and save the pixel data to disk
    let buffer_slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });

    let _ = device.poll(wgpu::PollType::wait_indefinitely());

    if let Ok(Ok(())) = rx.recv() {
        // FIX: Added unwrap() to handle the Result wrapping the mapped range view
        let data = buffer_slice
            .get_mapped_range()
            .expect("Failed to get mapped range view");

        // Remove padding to ensure normal pixel data alignment
        let mut clean_pixels = Vec::with_capacity((width * height * bytes_per_pixel) as usize);
        for chunk in data
            .chunks(padded_bytes_per_row as usize)
            .take(height as usize)
        {
            clean_pixels.extend_from_slice(&chunk[..unpadded_bytes_per_row as usize]);
        }

        std::fs::write("debug_frame_dump.raw", clean_pixels)
            .expect("Failed to write snapshot dump");
        println!("SUCCESS: Frame extracted directly to workspace as 'debug_frame_dump.raw'!");
    }
}

pub fn upload_frame_texture(
    queue: &Queue,
    app: &mut App,
    width: u32,
    height: u32,
    pixel_data: &[u8],
) {
    app.last_frame_width = width;
    app.last_frame_height = height;

    let texture_size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };

    if app
        .frame_texture
        .as_ref()
        .map_or(true, |t| t.width() != width || t.height() != height)
    {
        let device = app.device.as_ref().unwrap();
        app.frame_texture = Some(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Frame Texture"),
            size: texture_size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        }));
        app.frame_texture_view = Some(
            app.frame_texture
                .as_ref()
                .unwrap()
                .create_view(&Default::default()),
        );
    }

    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: app.frame_texture.as_ref().unwrap(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        pixel_data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        texture_size,
    );

    println!("Uploaded frame texture: {}x{}", width, height);

    let final_bind_group =
        app.device
            .as_ref()
            .unwrap()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Final Dynamic Bind Group"),
                layout: app.bind_group_layout.as_ref().unwrap(),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            app.frame_texture_view.as_ref().unwrap(),
                        ), // Real Frame Texture
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(app.sampler.as_ref().unwrap()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(
                            app.mask_texture_view.as_ref().unwrap(),
                        ), // Real Mask Texture
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: app.uniform_buffer.as_ref().unwrap().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        // Safely reads from the active app struct parameter available in this scope!
                        resource: app
                            .mask_storage_buffer
                            .as_ref()
                            .unwrap()
                            .as_entire_binding(),
                    },
                ],
            });

    // Assign the updated bind group block over the old placeholder struct handle
    app.bind_group = Some(final_bind_group);
}

pub fn run_mask_pass(
    encoder: &mut CommandEncoder,
    queue: &Queue,
    app: &mut App,
    actions: &[VisualAction],
) {
    if app.mask_texture_view.is_none()
        || app.mask_pipeline.is_none()
        || app.mask_bind_group.is_none()
    {
        return;
    }

    let mask_uniform = MaskUniforms {
        texture_size: glam::vec2(1280.0, 720.0),
    };

    // FIX: Clear and re-use our high-performance scratch pad vector
    app.uniform_scratch_pad.clear();
    let mut buffer_worker = encase::UniformBuffer::new(&mut app.uniform_scratch_pad);
    buffer_worker
        .write(&mask_uniform)
        .expect("Mask serialization failed");

    queue.write_buffer(
        app.mask_uniform_buffer.as_ref().unwrap(),
        0,
        &app.uniform_scratch_pad,
    );

    // Prepare action storage (remap: 0->1 blur, 1->2 blackbox)
    let active_source = if !actions.is_empty() {
        actions
    } else {
        &app.cached_actions
    };
    let mut raw_actions = [ActionInstance {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        action_type: 0,
        _pad: [0; 3],
    }; MAX_ACTIONS];
    for (i, act) in active_source.iter().enumerate().take(MAX_ACTIONS) {
        // Direct layout assignment matching inference.rs array layout!
        raw_actions[i] = ActionInstance {
            x: act.rect[0],      // Raw X percentage position
            y: act.rect[1],      // Raw Y percentage position
            width: act.rect[2],  // Raw Width percentage scale
            height: act.rect[3], // Raw Height percentage scale
            action_type: if act.action_type == 0 { 1 } else { 2 },
            _pad: [0; 3],
        };
        if i == 0 && !active_source.is_empty() {
            dbg!(&raw_actions[0]); // This prints the first rectangle to your console!
        }
    }

    queue.write_buffer(
        app.mask_storage_buffer.as_ref().unwrap(),
        0,
        bytemuck::cast_slice(&raw_actions),
    );

    let mut mask_pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("Mask Pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: app.mask_texture_view.as_ref().unwrap(),
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }), // Clean clear alpha base
                store: StoreOp::Store,
            },
            depth_slice: None,
        })],
        ..Default::default()
    });

    let action_count = actions.len().min(MAX_ACTIONS) as u32;
    if action_count > 0 {
        mask_pass.set_pipeline(app.mask_pipeline.as_ref().unwrap());
        mask_pass.set_bind_group(0, app.mask_bind_group.as_ref().unwrap(), &[]);
        mask_pass.draw(0..4, 0..action_count);
    }
    drop(mask_pass);
}

pub fn run_final_pass(
    encoder: &mut wgpu::CommandEncoder,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    render_pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    uniform_buffer: &wgpu::Buffer,
    output_view: &wgpu::TextureView,
    viewport_size: (u32, u32),
    screenshot_size: (u32, u32),
    scratch_pad: &mut Vec<u8>, // FIX: Accept the unified scratchpad parameter channel
) {
    let window_w = if viewport_size.0 == 0 {
        800.0
    } else {
        viewport_size.0 as f32
    };
    let window_h = if viewport_size.1 == 0 {
        600.0
    } else {
        viewport_size.1 as f32
    };
    let texture_w = if screenshot_size.0 == 0 {
        1280.0
    } else {
        screenshot_size.0 as f32
    };
    let texture_h = if screenshot_size.1 == 0 {
        720.0
    } else {
        screenshot_size.1 as f32
    };

    let final_uniform = FinalUniforms {
        texture_size: glam::vec2(texture_w, texture_h),
        viewport_size: glam::vec2(window_w, window_h),
    };

    scratch_pad.clear();
    let mut buffer_worker = encase::UniformBuffer::new(&mut *scratch_pad);
    buffer_worker
        .write(&final_uniform)
        .expect("Final layout serialization failed");

    queue.write_buffer(uniform_buffer, 0, scratch_pad);

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Final Composite Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: output_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });

        render_pass.set_pipeline(render_pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }
}
