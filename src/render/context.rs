use crate::shared_state::SharedAppState;

use ort::session::Session;
use std::sync::{Arc, Mutex};
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, Device, Queue, RenderPipeline, Sampler, Surface, Texture,
    TextureView,
};
use winit::window::Window;

// Uniform structs matching shader
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ActionInstance {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub action_type: u32, // 1 = blur, 2 = blackbox
    pub _pad1: u32,
    pub _pad2: u32,
    pub _pad3: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MaskUniforms {
    pub viewport_width: f32,
    pub viewport_height: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct FinalUniforms {
    pub viewport_width: f32,
    pub viewport_height: f32,
}

pub struct App {
    pub window: Option<Arc<Window>>,
    pub surface: Option<Surface<'static>>,
    pub device: Option<Device>,
    pub queue: Option<Queue>,
    pub state: Arc<SharedAppState>,
    pub frame_texture: Option<Texture>,
    pub frame_texture_view: Option<TextureView>,
    pub sampler: Option<Sampler>,
    pub sessions: Vec<Arc<Mutex<Session>>>,
    pub pipeline: Option<RenderPipeline>, // final compositing
    pub bind_group_layout: Option<BindGroupLayout>,
    pub uniform_buffer: Option<Buffer>, // final uniform
    pub bind_group: Option<BindGroup>,
    pub viewport_size: (u32, u32),

    // Mask rendering resources
    pub mask_texture: Option<Texture>,
    pub mask_texture_view: Option<TextureView>,
    pub mask_pipeline: Option<RenderPipeline>,
    pub mask_bind_group_layout: Option<BindGroupLayout>,
    pub mask_bind_group: Option<BindGroup>,
    pub mask_uniform_buffer: Option<Buffer>,
    pub mask_storage_buffer: Option<Buffer>,
}

impl App {
    pub fn new(state: Arc<SharedAppState>, sessions: Vec<Arc<Mutex<Session>>>) -> Self {
        Self {
            window: None,
            surface: None,
            device: None,
            queue: None,
            state,
            frame_texture: None,
            frame_texture_view: None,
            sampler: None,
            sessions,
            pipeline: None,
            bind_group_layout: None,
            uniform_buffer: None,
            bind_group: None,
            viewport_size: (1, 1),
            mask_texture: None,
            mask_texture_view: None,
            mask_pipeline: None,
            mask_bind_group_layout: None,
            mask_bind_group: None,
            mask_uniform_buffer: None,
            mask_storage_buffer: None,
        }
    }
}
