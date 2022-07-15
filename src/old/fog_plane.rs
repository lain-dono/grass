use super::EntityPipeline;
use bytemuck::{Pod, Zeroable};
use std::mem::size_of;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Params {
    transform: [f32; 16],
    tint: [f32; 4], // 1,1,1,0.5
    inv_size: [f32; 2],
    strength: f32, // 0.5 (0..3)
    znear: f32,
}

pub struct QuadBuffer {
    vtx: wgpu::Buffer,
    idx: wgpu::Buffer,
}

impl QuadBuffer {
    fn new(device: &wgpu::Device, size: i8) -> Self {
        let (p, n) = (size, -size);

        let vtx = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad vertices"),
            contents: bytemuck::cast_slice(&[
                [p, 0, n, 1],
                [p, 0, p, 1],
                [n, 0, n, 1],
                [n, 0, p, 1],
            ]),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let idx = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("quad indices"),
            contents: bytemuck::cast_slice(&[0u16, 1, 2, 2, 1, 3]),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self { vtx, idx }
    }
}

pub struct FogPlanePipeline {
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
    pub layout: wgpu::BindGroupLayout,
    pub buffer: wgpu::Buffer,
    pub quad: QuadBuffer,
}

impl FogPlanePipeline {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        depth: &wgpu::TextureView,
        sample_count: u32,
    ) -> Self {
        let source = std::fs::read_to_string("src/fog_plane.wgsl").unwrap();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let multisampled = sample_count > 1;

        let params_size = size_of::<Params>() as wgpu::BufferAddress;

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(params_size),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("fog plane"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let vb_desc = wgpu::VertexBufferLayout {
            array_stride: size_of::<[i8; 4]>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Sint8x4],
        };

        // Create the render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fog plane"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: "vs_main",
                buffers: &[vb_desc],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: if multisampled {
                    "fs_main_multi"
                } else {
                    "fs_main_single"
                },
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    //blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::all(),
                })],
            }),
            primitive: wgpu::PrimitiveState {
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            //depth_stencil: None,
            depth_stencil: Some(wgpu::DepthStencilState {
                format: EntityPipeline::SHADOW_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState {
                    constant: 2, // corresponds to bilinear filtering
                    slope_scale: 2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState {
                count: sample_count,
                ..Default::default()
            },
            multiview: None,
        });

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::bytes_of(&Params::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = create_bind_group(device, &layout, &buffer, depth);

        Self {
            layout,
            pipeline,
            buffer,
            bind_group,
            quad: QuadBuffer::new(device, 5),
        }
    }

    pub fn update(
        &mut self,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        view_proj: glam::Mat4,
        znear: f32,
    ) {
        let transform = glam::Mat4::from_translation(glam::vec3(0.0, 2.0, 0.0));
        let uniforms = Params {
            transform: (view_proj * transform).to_cols_array(),
            tint: [0.0, 0.0, 0.15, 0.5],
            inv_size: [(width as f32).recip(), (height as f32).recip()],
            strength: 2.95, // 0.15
            znear,
        };
        queue.write_buffer(&self.buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    pub fn resize(&mut self, device: &wgpu::Device, depth: &wgpu::TextureView) {
        self.bind_group = create_bind_group(device, &self.layout, &self.buffer, depth);
    }

    pub fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        resolve_target: Option<&wgpu::TextureView>,
        depth: &wgpu::TextureView,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: None,
                stencil_ops: None,
            }),
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_index_buffer(self.quad.idx.slice(..), wgpu::IndexFormat::Uint16);
        pass.set_vertex_buffer(0, self.quad.vtx.slice(..));
        pass.draw_indexed(0..6, 0, 0..1);
    }
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    params_buf: &wgpu::Buffer,
    depth: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(depth),
            },
        ],
    })
}