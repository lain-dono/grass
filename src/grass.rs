use super::EntityPipeline;
use std::mem::size_of;
use std::time::Instant;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Default, Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Params {
    pub time: f32,
    pub length: u32,
    pub blades: u32,

    pub blade_radius: f32,
    pub blade_forward: f32,
    pub blade_curve: f32,

    pub wind_speed: f32,
    pub wind_strength: f32,
}

pub struct GrassPipeline {
    pub compute_layout: wgpu::BindGroupLayout,

    pub init: wgpu::ComputePipeline,
    pub fill: wgpu::ComputePipeline,
    pub bake: wgpu::RenderPipeline,
    pub draw: wgpu::RenderPipeline,
}

impl GrassPipeline {
    pub const WORKGROUPS: u32 = 256;

    pub fn new(
        device: &wgpu::Device,
        entity: &EntityPipeline,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let source = std::fs::read_to_string("src/grass.wgsl").unwrap();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grass"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, // params
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<Params>() as u64),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, // src_vertices
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, // dst_vertices
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3, // dst_vertices
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4, // dst_indirect
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            size_of::<DrawIndexedIndirect>() as u64
                        ),
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grass_compute"),
            bind_group_layouts: &[&compute_layout],
            push_constant_ranges: &[],
        });

        let init = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("grass_compute_init"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: "cs_main_init",
        });

        let fill = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("grass_compute_fill"),
            layout: Some(&pipeline_layout),
            module: &module,
            entry_point: "cs_main_fill",
        });

        // rendering

        let vb_desc = wgpu::VertexBufferLayout {
            array_stride: (size_of::<DstVertex>()) as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
        };

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grass_bake"),
            bind_group_layouts: &[
                &entity.shadow_bind_group_layout,
                &entity.entity_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let bake = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grass_bake"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: "vs_bake",
                buffers: &[vb_desc.clone()],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: Some(wgpu::IndexFormat::Uint32),
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: device
                    .features()
                    .contains(wgpu::Features::DEPTH_CLIP_CONTROL),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: EntityPipeline::SHADOW_FORMAT,
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grass_draw"),
            bind_group_layouts: &[
                &entity.forward_bind_group_layout,
                &entity.entity_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let draw = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grass_draw"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: "vs_draw",
                buffers: &[vb_desc],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "fs_draw",
                targets: &[Some(format.into())],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: Some(wgpu::IndexFormat::Uint32),
                front_face: wgpu::FrontFace::Ccw,
                //cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: EntityPipeline::DEPTH_FORMAT,
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
            init,
            fill,
            compute_layout,
            draw,
            bake,
        }
    }

    pub fn dispatch(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        bind_group: &wgpu::BindGroup,
        count: u32,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_bind_group(0, bind_group, &[]);

        pass.set_pipeline(&self.init);
        pass.dispatch_workgroups(1, 1, 1);
        pass.set_pipeline(&self.fill);
        pass.dispatch_workgroups(count / Self::WORKGROUPS + count % Self::WORKGROUPS, 1, 1);
    }
}

pub struct Grass {
    pub pipeline: GrassPipeline,

    pub bind_group: wgpu::BindGroup,
    pub params_buf: wgpu::Buffer,
    pub src_vertices_buf: wgpu::Buffer,
    pub src_vertices_len: usize,
    pub dst_vertices_buf: wgpu::Buffer,
    pub dst_vertices_len: wgpu::Buffer,
    pub dst_indirect_buf: wgpu::Buffer,
    pub indices_buf: wgpu::Buffer,

    pub start: Instant,
}

impl Grass {
    pub fn new(
        device: &wgpu::Device,
        entity: &EntityPipeline,
        format: wgpu::TextureFormat,
        sample_count: u32,
    ) -> Self {
        let pipeline = GrassPipeline::new(device, entity, format, sample_count);

        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: bytemuck::bytes_of(&Params::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        //let src_vertices = create_source(30, 30, 0.25);
        let src_vertices = create_source(100, 100, 0.25);
        let src_vertices_len = src_vertices.len();
        let src_vertices_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("src_vertices"),
            contents: bytemuck::cast_slice(&src_vertices),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
        });

        let segments = 5;
        let blades = 5;
        let vertices_count = src_vertices_len * (segments * 2 + 1) * blades;
        let blades_count = src_vertices_len * blades;

        let dst_vertices_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("dst_vertices"),
            size: (vertices_count * size_of::<DstVertex>()) as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::VERTEX,
            mapped_at_creation: false,
        });

        let dst_vertices_len = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("dst_vertices_count"),
            contents: bytemuck::bytes_of(&[0u32; 4]),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let dst_indirect_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("dst_draw_indirect"),
            contents: bytemuck::bytes_of(&DrawIndexedIndirect {
                vertex_count: 0,
                instance_count: 1,
                base_index: 0,
                vertex_offset: 0,
                base_instance: 0,
            }),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::INDIRECT,
        });

        let indices = indices(blades_count as u32, segments as u32);
        let indices_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &pipeline.compute_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: src_vertices_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: dst_vertices_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: dst_vertices_len.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: dst_indirect_buf.as_entire_binding(),
                },
            ],
            label: None,
        });

        Self {
            pipeline,

            params_buf,

            src_vertices_buf,
            src_vertices_len,

            dst_vertices_buf,
            dst_vertices_len,
            dst_indirect_buf,

            bind_group,

            indices_buf,

            start: Instant::now(),
        }
    }

    pub fn update(&self, queue: &wgpu::Queue) {
        let time = self.start.elapsed();

        let params = Params {
            time: time.as_secs_f32(),
            length: self.src_vertices_len as u32,

            blades: 5,
            blade_radius: 0.392,
            blade_forward: 0.38,
            blade_curve: 2.1,

            wind_speed: 3.0,
            wind_strength: 0.10, // 0.05
        };

        queue.write_buffer(&self.params_buf, 0, bytemuck::bytes_of(&params));
    }

    pub fn dispatch(&self, encoder: &mut wgpu::CommandEncoder) {
        self.pipeline
            .dispatch(encoder, &self.bind_group, self.src_vertices_len as u32);
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndirect {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub base_vertex: u32,
    pub base_instance: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DrawIndexedIndirect {
    pub vertex_count: u32,
    pub instance_count: u32,
    pub base_index: u32,
    pub vertex_offset: i32,
    pub base_instance: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SrcVertex {
    position: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DstVertex {
    position: [f32; 3],
    normal: [f32; 3],
    texcoord: [f32; 2],
}

fn create_source(x_size: usize, z_size: usize, scale: f32) -> Vec<SrcVertex> {
    let (x_size, z_size) = (x_size * 2, z_size * 2);
    let capacity = (x_size + 1) * (z_size + 1);

    let mut vertices = Vec::with_capacity(capacity);

    let hx = x_size as i32 / 2;
    let hz = z_size as i32 / 2;
    for z in 0..=z_size as i32 {
        for x in 0..=x_size as i32 {
            let (x, z) = (x - hx, z - hz);
            vertices.push(SrcVertex {
                position: [x as f32 * scale, 0.0, z as f32 * scale],
                normal: [0.0, 1.0, 0.0],
            });
        }
    }

    vertices
}

fn indices(blades: u32, segments: u32) -> Vec<u32> {
    let vertices_per_blade = segments * 2 + 1;
    let capacity = blades * (vertices_per_blade + 1);

    let mut indices = Vec::with_capacity(capacity as usize);
    for blade in 0..blades {
        let start = vertices_per_blade * blade;
        indices.extend((0..vertices_per_blade).map(|i| start + i));
        indices.push(u32::MAX); // reset strip
    }

    assert_eq!(indices.capacity(), indices.len());

    indices
}
