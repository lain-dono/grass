use bytemuck::{Pod, Zeroable};
use std::mem::size_of;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GlobalUniforms {
    pub proj: [[f32; 4]; 4],
    pub num_lights: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct EntityUniforms {
    pub model: [[f32; 4]; 4],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct LightRaw {
    pub proj: [[f32; 4]; 4],
    pub pos: [f32; 4],
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [i8; 4],
    pub normal: [i8; 4],
}

pub struct EntityPipeline {
    pub entity_bind_group_layout: wgpu::BindGroupLayout,
    pub shadow_bind_group_layout: wgpu::BindGroupLayout,
    pub forward_bind_group_layout: wgpu::BindGroupLayout,
    pub bake: wgpu::RenderPipeline,
    pub draw: wgpu::RenderPipeline,
}

impl EntityPipeline {
    pub const MAX_LIGHTS: usize = 10;
    pub const SHADOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, sample_count: u32) -> Self {
        let source = std::fs::read_to_string("src/entity.wgsl").unwrap();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let entity_uniform_size = size_of::<EntityUniforms>() as wgpu::BufferAddress;
        let vertex_size = size_of::<Vertex>();

        let light_uniform_size = (Self::MAX_LIGHTS * size_of::<LightRaw>()) as wgpu::BufferAddress;

        let vb_desc = wgpu::VertexBufferLayout {
            array_stride: vertex_size as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Sint8x4, 1 => Sint8x4],
        };

        let local_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(entity_uniform_size),
                    },
                    count: None,
                }],
                label: None,
            });

        let shadow_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0, // global
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            size_of::<GlobalUniforms>() as wgpu::BufferAddress
                        ),
                    },
                    count: None,
                }],
            });

        let forward_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: None,
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0, // global
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(
                                size_of::<GlobalUniforms>() as wgpu::BufferAddress
                            ),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1, // lights
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(light_uniform_size),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            sample_type: wgpu::TextureSampleType::Depth,
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                        count: None,
                    },
                ],
            });

        let bake_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bake"),
            bind_group_layouts: &[&shadow_bind_group_layout, &local_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create the render pipeline
        let bake = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bake"),
            layout: Some(&bake_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: "vs_bake",
                buffers: &[vb_desc.clone()],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: device
                    .features()
                    .contains(wgpu::Features::DEPTH_CLIP_CONTROL),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Self::SHADOW_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2, // corresponds to bilinear filtering
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let draw_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("draw"),
            bind_group_layouts: &[&forward_bind_group_layout, &local_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create the render pipeline
        let draw = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("draw"),
            layout: Some(&draw_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: "vs_draw",
                buffers: &[vb_desc],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "fs_draw",
                targets: &[Some(format.into()), Some(crate::Framebuffer::NORMAL.into())],
            }),
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: Self::DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                ..Default::default()
            },
            multiview: None,
        });

        Self {
            entity_bind_group_layout: local_bind_group_layout,
            shadow_bind_group_layout,
            forward_bind_group_layout,
            bake,
            draw,
        }
    }
}
