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

    // Update mask uniform
    let mask_uniform = MaskUniforms {
        viewport_width: app.viewport_size.0 as f32,
        viewport_height: app.viewport_size.1 as f32,
    };
    queue.write_buffer(
        app.mask_uniform_buffer.as_ref().unwrap(),
        0,
        bytemuck::cast_slice(&[mask_uniform]),
    );

    // Prepare action storage (remap: 0->1 blur, 1->2 blackbox)
    let mut raw_actions = [ActionInstance {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        action_type: 0,
        _pad1: 0,
        _pad2: 0,
        _pad3: 0,
    }; MAX_ACTIONS];
    for (i, act) in actions.iter().enumerate().take(MAX_ACTIONS) {
        raw_actions[i] = ActionInstance {
            x: act.rect[0],
            y: act.rect[1],
            width: act.rect[2],
            height: act.rect[3],
            action_type: if act.action_type == 0 { 1 } else { 2 },
            _pad1: 0,
            _pad2: 0,
            _pad3: 0,
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
                    a: 1.0,
                }),
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
    encoder: &mut CommandEncoder,
    queue: &Queue,
    app: &mut App,
    surface_texture: &wgpu::SurfaceTexture,
) -> Result<(), ()> {
    let view = surface_texture.texture.create_view(&Default::default());

    // Update final bind group with current textures
    let frame_view = app.frame_texture_view.as_ref().unwrap();
    let mask_view = app.mask_texture_view.as_ref().unwrap();
    let sampler = app.sampler.as_ref().unwrap();
    let uniform = app.uniform_buffer.as_ref().unwrap();

    let final_bind_group =
        app.device
            .as_ref()
            .unwrap()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Final Bind Group"),
                layout: app.bind_group_layout.as_ref().unwrap(),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(frame_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(mask_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            });
    app.bind_group = Some(final_bind_group);

    // Update final uniform (viewport)
    let final_uniform = FinalUniforms {
        viewport_width: app.viewport_size.0 as f32,
        viewport_height: app.viewport_size.1 as f32,
    };
    queue.write_buffer(
        app.uniform_buffer.as_ref().unwrap(),
        0,
        bytemuck::cast_slice(&[final_uniform]),
    );

    let mut final_pass = encoder.begin_render_pass(&RenderPassDescriptor {
        label: Some("Final Pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &view,
            resolve_target: None,
            ops: Operations {
                load: LoadOp::Clear(Color {
                    r: 0.1,
                    g: 0.1,
                    b: 0.1,
                    a: 1.0,
                }),
                store: StoreOp::Store,
            },
            depth_slice: None,
        })],
        ..Default::default()
    });

    final_pass.set_pipeline(app.pipeline.as_ref().unwrap());
    final_pass.set_bind_group(0, app.bind_group.as_ref().unwrap(), &[]);
    final_pass.draw(0..4, 0..1);

    Ok(())
}
