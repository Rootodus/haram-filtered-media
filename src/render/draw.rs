use super::context::{ActionInstance, App, FinalUniforms, MaskUniforms};
use crate::protocol::{MAX_ACTIONS, VisualAction};

use bytemuck;
use wgpu::{
    Color, CommandEncoder, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, TextureFormat,
};

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

    // FIX: Pack directly into our newly refactored glam matrix layout
    let mask_uniform = MaskUniforms {
        texture_size: glam::vec2(app.last_frame_width as f32, app.last_frame_height as f32),
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
    let mut raw_actions = [ActionInstance {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        action_type: 0,
        _pad: [0; 3],
    }; MAX_ACTIONS];
    for (i, act) in actions.iter().enumerate().take(MAX_ACTIONS) {
        raw_actions[i] = ActionInstance {
            x: act.rect[0],
            y: act.rect[1],
            width: act.rect[2],
            height: act.rect[3],
            action_type: if act.action_type == 0 { 1 } else { 2 },
            _pad: [0; 3],
        };
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
}

pub fn run_final_pass(
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

    let window_aspect = window_w / window_h;
    let texture_aspect = texture_w / texture_h;

    let (scale, offset) = if window_aspect > texture_aspect {
        let s = texture_aspect / window_aspect;
        (glam::vec2(s, 1.0), glam::vec2((1.0 - s) * 0.5, 0.0))
    } else {
        let s = window_aspect / texture_aspect;
        (glam::vec2(1.0, s), glam::vec2(0.0, (1.0 - s) * 0.5))
    };

    let final_uniform = FinalUniforms {
        uv_scale: scale,
        uv_offset: offset,
        texture_size: glam::vec2(texture_w, texture_h),
        viewport_size: glam::vec2(window_w, window_h),
    };

    scratch_pad.clear();
    let mut buffer_worker = encase::UniformBuffer::new(&mut *scratch_pad);
    buffer_worker
        .write(&final_uniform)
        .expect("Final layout serialization failed");

    queue.write_buffer(uniform_buffer, 0, scratch_pad);

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Final Composite Encoder"),
    });

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

    queue.submit(std::iter::once(encoder.finish()));
}
