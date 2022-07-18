use bevy::{
    core_pipeline::core_3d::Opaque3d,
    ecs::{query::QueryItem, system::lifetimeless::Read},
    prelude::*,
    render::{extract_component::ExtractComponent, render_phase::AddRenderCommand},
    render::{
        extract_component::ExtractComponentPlugin,
        extract_resource::{ExtractResource, ExtractResourcePlugin},
        main_graph::node::CAMERA_DRIVER,
        render_graph::RenderGraph,
        render_resource::*,
        renderer::{RenderDevice, RenderQueue},
        RenderApp, RenderStage,
    },
};
use bytemuck::{Pod, Zeroable};
use std::mem::size_of;

mod compute;
mod render;

pub use self::compute::{GrassComputeNode, GrassComputePipeline};
pub use self::render::{DrawGrass, GrassRenderPipeline};

pub struct GrassPlugin;

impl Plugin for GrassPlugin {
    fn build(&self, app: &mut App) {
        let render_device = app.world.resource::<RenderDevice>();
        let buffer = GrassData::new(render_device);

        app.add_plugin(ExtractComponentPlugin::<Grass>::default());
        app.add_plugin(ExtractResourcePlugin::<ExtractedTime>::default());

        let render_app = app.sub_app_mut(RenderApp);
        render_app
            .insert_resource(buffer)
            .add_render_command::<Opaque3d, DrawGrass>()
            //.add_render_command::<super::normal_pass::Normal3d, DrawGrass>()
            .init_resource::<GrassComputePipeline>()
            .init_resource::<GrassRenderPipeline>()
            .init_resource::<SpecializedRenderPipelines<GrassRenderPipeline>>()
            .add_system_to_stage(RenderStage::Prepare, ExtractedTime::prepare)
            .add_system_to_stage(RenderStage::Extract, self::render::extract_grass)
            .add_system_to_stage(RenderStage::Queue, self::render::queue_grass)
            .add_system_to_stage(RenderStage::Queue, queue_bind_group);

        let mut render_graph = render_app.world.resource_mut::<RenderGraph>();
        render_graph.add_node("grass", GrassComputeNode::default());
        render_graph.add_node_edge("grass", CAMERA_DRIVER).unwrap();
    }
}

#[derive(Default, Clone, Copy, Component)]
pub struct Grass;

impl ExtractComponent for Grass {
    type Query = Read<Self>;
    type Filter = ();

    #[inline]
    fn extract_component(_item: QueryItem<Self::Query>) -> Self {
        Self
    }
}

#[derive(Default)]
struct ExtractedTime {
    seconds_since_startup: f32,
}

impl ExtractedTime {
    // write the extracted time into the corresponding uniform buffer
    fn prepare(time: Res<ExtractedTime>, data: Res<GrassData>, render_queue: Res<RenderQueue>) {
        render_queue.write_buffer(
            &data.params_buf,
            0,
            bevy::core::cast_slice(&[time.seconds_since_startup]),
        );
    }
}

impl ExtractResource for ExtractedTime {
    type Source = Time;

    fn extract_resource(time: &Self::Source) -> Self {
        Self {
            seconds_since_startup: time.seconds_since_startup() as f32,
        }
    }
}

const WORKGROUPS: u32 = 256;

pub struct GrassConfig {
    pub blades: u32,
    pub blade_radius: f32,
    pub blade_forward: f32,
    pub blade_curve: f32,
    pub wind_speed: f32,
    pub wind_strength: f32,
}

impl Default for GrassConfig {
    fn default() -> Self {
        Self {
            blades: 5,
            blade_radius: 0.392,
            blade_forward: 0.38,
            blade_curve: 2.1,

            wind_speed: 1.0,
            wind_strength: 0.015,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GrassUniform {
    time: f32,
    length: u32,

    blades: u32,

    blade_radius: f32,
    blade_forward: f32,
    blade_curve: f32,

    wind_speed: f32,
    wind_strength: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct DstVertex {
    position: [f32; 3],
    normal: [f32; 3],
    texcoord: [f32; 2],
}

#[derive(Default, Bundle)]
pub struct GrassBundle {
    pub grass: Grass,
    pub transform: Transform,
    pub global_transform: GlobalTransform,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GrassSourceVertex {
    position: [f32; 3],
    normal: [f32; 3],
}

pub struct GrassData {
    pub params_buf: Buffer,

    pub src_vertices_buf: Buffer,
    pub src_vertices_len: usize,

    pub vertex_buffer_len: Buffer,

    pub vertex_buffer: Buffer,
    pub index_buffer: Buffer,
    pub indirect_buffer: Buffer,
}

impl GrassData {
    fn new(device: &RenderDevice) -> Self {
        fn create_source(x_size: usize, z_size: usize, scale: f32) -> Vec<GrassSourceVertex> {
            let (x_size, z_size) = (x_size * 2, z_size * 2);
            let capacity = (x_size + 1) * (z_size + 1);

            let mut vertices = Vec::with_capacity(capacity);

            let hx = x_size as i32 / 2;
            let hz = z_size as i32 / 2;
            for z in 0..=z_size as i32 {
                for x in 0..=x_size as i32 {
                    let (x, z) = (x - hx, z - hz);
                    vertices.push(GrassSourceVertex {
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

        let src_vertices = create_source(50, 50, 0.05);
        let src_vertices_len = src_vertices.len();
        let src_vertices_buf = device.create_buffer_with_data(&wgpu::util::BufferInitDescriptor {
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

        let dst_vertices_len = device.create_buffer_with_data(&wgpu::util::BufferInitDescriptor {
            label: Some("dst_vertices_count"),
            contents: bytemuck::bytes_of(&[0u32; 4]),
            usage: wgpu::BufferUsages::STORAGE,
        });

        let dst_indirect_buf = device.create_buffer_with_data(&wgpu::util::BufferInitDescriptor {
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
        let indices_buf = device.create_buffer_with_data(&wgpu::util::BufferInitDescriptor {
            label: Some("indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let config = GrassConfig::default();
        let params_buf = device.create_buffer_with_data(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: bytemuck::bytes_of(&GrassUniform {
                time: 123.456,
                length: src_vertices_len as u32,

                blades: config.blades,

                blade_radius: config.blade_radius,
                blade_forward: config.blade_forward,
                blade_curve: config.blade_curve,

                wind_speed: config.wind_speed,
                wind_strength: config.wind_strength,
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            params_buf,

            src_vertices_buf,
            src_vertices_len,

            vertex_buffer: dst_vertices_buf,
            vertex_buffer_len: dst_vertices_len,
            indirect_buffer: dst_indirect_buf,

            index_buffer: indices_buf,
        }
    }
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

pub struct GrassBindGroup {
    bind_group: BindGroup,
    count: u32,
}

fn queue_bind_group(
    mut commands: Commands,
    pipeline: Res<GrassComputePipeline>,
    device: Res<RenderDevice>,
    data: Res<GrassData>,
) {
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.compute_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: data.params_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: data.src_vertices_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: data.vertex_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: data.vertex_buffer_len.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: data.indirect_buffer.as_entire_binding(),
            },
        ],
    });

    let count = data.src_vertices_len as u32;
    let count = count / WORKGROUPS + count % WORKGROUPS;
    commands.insert_resource(GrassBindGroup { bind_group, count });
}
