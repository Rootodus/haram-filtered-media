use super::context::FinalUniforms;

use encase::ShaderType;
use wgpu::{
    BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BufferBindingType, ColorTargetState, ColorWrites, Device, FrontFace, MultisampleState,
    PipelineCompilationOptions, PipelineLayoutDescriptor, PolygonMode, PrimitiveState,
    RenderPipeline, RenderPipelineDescriptor, SamplerBindingType, ShaderStages, TextureFormat,
    TextureSampleType, TextureViewDimension, include_wgsl,
};

pub struct Pipelines {
    pub mask_pipeline: RenderPipeline,
    pub final_pipeline: RenderPipeline,
    pub mask_bind_group_layout: BindGroupLayout,
    pub final_bind_group_layout: BindGroupLayout,
}

pub fn create_pipelines(device: &Device) -> Pipelines {
    let shader = device.create_shader_module(include_wgsl!("shaders/mask_effects.wgsl"));

    // ---------- Mask bind group layout ----------
    let mask_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("Mask Bind Group Layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    // FIX: Enforce compile-time safety bounds computed by encase
                    min_binding_size: Some(super::context::MaskUniforms::min_size()),
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::VERTEX,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    // ---------- Final bind group layout ----------
    let final_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
        label: Some("Final Bind Group Layout"),
        entries: &[
            BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Sampler(SamplerBindingType::Filtering),
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 2,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Texture {
                    sample_type: TextureSampleType::Float { filterable: true },
                    view_dimension: TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            BindGroupLayoutEntry {
                binding: 3,
                visibility: ShaderStages::FRAGMENT,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: Some(FinalUniforms::min_size()),
                },
                count: None,
            },
            // ====================================================================
            // ADD THIS ENTRY AT THE END: Binding 4 for storage data inspection
            // ====================================================================
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    // ---------- Mask pipeline ----------
    let mask_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Mask Pipeline Layout"),
        bind_group_layouts: &[Some(&mask_bind_group_layout)],
        immediate_size: 0,
    });

    let mask_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Mask Pipeline"),
        layout: Some(&mask_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_mask"),
            buffers: &[],
            compilation_options: PipelineCompilationOptions::default(),
        },
        primitive: PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_mask"),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::R8Unorm,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    });

    // ---------- Final pipeline ----------
    let final_pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("Final Pipeline Layout"),
        bind_group_layouts: &[Some(&final_bind_group_layout)],
        immediate_size: 0,
    });

    let final_pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
        label: Some("Final Pipeline"),
        layout: Some(&final_pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: PipelineCompilationOptions::default(),
        },
        primitive: PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(ColorTargetState {
                format: TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            compilation_options: PipelineCompilationOptions::default(),
        }),
        multiview_mask: None,
        cache: None,
    });

    Pipelines {
        mask_pipeline,
        final_pipeline,
        mask_bind_group_layout,
        final_bind_group_layout,
    }
}
