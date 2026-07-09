use crate::shared_state::SharedAppState;
use crate::types::VisualAction;

use encase::ShaderType;
use glam::Vec2;
use ort::session::Session;
use std::sync::{Arc, Mutex};
use wgpu::{
    BindGroup, BindGroupLayout, Buffer, Device, Queue, RenderPipeline, Sampler, Surface, Texture,
    TextureView,
};
use winit::window::Window;

// Uniform structs matching shader
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ActionInstance {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub action_type: u32, // 1 = blur, 2 = blackbox
    pub _pad: [u32; 3],
}

#[derive(Clone, Copy, ShaderType)]
pub struct MaskUniforms {
    pub texture_size: Vec2,
}

#[derive(Clone, Copy, ShaderType)]
pub struct FinalUniforms {
    pub texture_size: Vec2,
    pub viewport_size: Vec2,
}

#[derive(Debug)]
pub enum CustomAppEvent {
    RequestShutdown,
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
    pub uniform_scratch_pad: Vec<u8>,
    pub cached_actions: Vec<VisualAction>,

    // Mask rendering resources
    pub mask_texture: Option<Texture>,
    pub mask_texture_view: Option<TextureView>,
    pub mask_pipeline: Option<RenderPipeline>,
    pub mask_bind_group_layout: Option<BindGroupLayout>,
    pub mask_bind_group: Option<BindGroup>,
    pub mask_uniform_buffer: Option<Buffer>,
    pub mask_storage_buffer: Option<Buffer>,

    pub last_frame_width: u32,
    pub last_frame_height: u32,
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
            uniform_scratch_pad: Vec::with_capacity(64),
            cached_actions: Vec::new(),
            mask_texture: None,
            mask_texture_view: None,
            mask_pipeline: None,
            mask_bind_group_layout: None,
            mask_bind_group: None,
            mask_uniform_buffer: None,
            mask_storage_buffer: None,
            last_frame_width: 0,
            last_frame_height: 0,
        }
    }
}
