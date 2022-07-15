use bytemuck::{Pod, Zeroable};
use std::mem::size_of;
use std::time::Instant;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Params {
    pub view_space_directon: [f32; 4],
    pub color: [f32; 4],

    /// Number of pixels between samples that are tested for an edge.
    /// When this value is 1, tested samples are adjacent.
    pub scale: i32,

    pad: [f32; 3],

    /// Difference between depth values, scaled by the current depth, required to draw an edge.
    pub depth_threshold: f32, // 0..1

    /// The value at which the dot product between the surface normal
    /// and the view direction will affect the depth threshold.
    /// This ensures that surfaces at right angles to the camera
    /// require a larger depth threshold to draw an edge,
    /// avoiding edges being drawn along slopes.
    pub depth_normal_threshold: f32,

    /// Scale the strength of how much the depth_normal_threshold affects the depth threshold.
    pub depth_normal_threshold_scale: f32,

    /// Larger values will require the difference between normals to be greater to draw an edge.
    pub normal_threshold: f32, // 0..1
}

pub struct Postprocess {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,

    pub pipeline: wgpu::RenderPipeline,
    pub uniform_buf: wgpu::Buffer,

    pub start: Instant,
}

impl Postprocess {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        depth: &wgpu::TextureView,
        normal: &wgpu::TextureView,
        sample_count: u32,
    ) -> Self {
        let source = std::fs::read_to_string("src/post.wgsl").unwrap();
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });

        let multisampled = sample_count > 1;

        let uniform_size = size_of::<Params>() as wgpu::BufferAddress;

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(uniform_size),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled,
                    },
                    count: None,
                },
            ],
            label: None,
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("postprocess"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create the render pipeline
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("postprocess"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: "vs_fullscreen",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: if multisampled {
                    "fs_main_multi"
                } else {
                    "fs_main_single"
                },
                targets: &[Some(wgpu::ColorTargetState {
                    //format.into()
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
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                ..Default::default()
            },
            multiview: None,
        });

        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Uniform Buffer"),
            contents: bytemuck::bytes_of(&Params::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group = create_bind_group(device, &bind_group_layout, &uniform_buf, depth, normal);

        Self {
            bind_group_layout,
            pipeline,
            uniform_buf,
            bind_group,
            start: Instant::now(),
        }
    }

    pub fn update(&mut self, queue: &wgpu::Queue, view_space_directon: glam::Vec3) {
        let uniforms = Params {
            //size: [width, height],
            //inv_size: [width.recip(), height.recip()],
            //time: self.start.elapsed().as_secs_f32(),
            view_space_directon: view_space_directon.extend(0.0).into(),
            color: [0.02, 0.00, 0.00, 0.25],
            scale: 0,
            depth_threshold: 1.5,
            depth_normal_threshold: 0.5,
            depth_normal_threshold_scale: 7.0,
            normal_threshold: 0.4,

            ..Params::zeroed()
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));
    }

    pub fn resize(
        &mut self,
        device: &wgpu::Device,
        depth: &wgpu::TextureView,
        normal: &wgpu::TextureView,
    ) {
        self.bind_group = create_bind_group(
            device,
            &self.bind_group_layout,
            &self.uniform_buf,
            depth,
            normal,
        );
    }

    pub fn draw(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                },
            })],
            depth_stencil_attachment: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn create_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform_buf: &wgpu::Buffer,
    depth: &wgpu::TextureView,
    normal: &wgpu::TextureView,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(depth),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(normal),
            },
        ],
    })
}
