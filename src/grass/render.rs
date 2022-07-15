use bevy::{
    core_pipeline::core_3d::Opaque3d,
    ecs::system::{lifetimeless::SRes, SystemParamItem},
    pbr::{MeshPipeline, MeshUniform, SetMeshBindGroup, SetMeshViewBindGroup},
    prelude::*,
    render::render_phase::{
        DrawFunctions, EntityRenderCommand, RenderCommandResult, RenderPhase, SetItemPipeline,
        TrackedRenderPass,
    },
    render::texture::BevyDefault,
    render::{render_resource::*, Extract},
};

use super::{Grass, GrassData};

pub fn extract_grass(
    mut commands: Commands,
    query: Extract<Query<(Entity, &GlobalTransform), With<Grass>>>,
) {
    for (entity, transform) in query.iter() {
        let transform = transform.compute_matrix();
        commands.get_or_spawn(entity).insert(MeshUniform {
            flags: 1,
            transform,
            inverse_transpose_model: transform.inverse().transpose(),
        });
    }
}

pub fn queue_grass(
    pipeline: Res<GrassRenderPipeline>,
    draw_functions: Res<DrawFunctions<Opaque3d>>,
    msaa: Res<Msaa>,
    mut pipelines: ResMut<SpecializedRenderPipelines<GrassRenderPipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
    mut view_query: Query<&mut RenderPhase<Opaque3d>>,
    query: Query<Entity, With<Grass>>,
) {
    let draw_function = draw_functions.read().get_id::<DrawGrass>().unwrap();

    for mut opaque_phase in view_query.iter_mut() {
        for entity in query.iter() {
            let key = GrassPipelineKey::from_msaa_samples(msaa.samples);

            let pipeline = pipelines.specialize(&mut pipeline_cache, &pipeline, key);

            opaque_phase.add(Opaque3d {
                entity,
                pipeline,
                draw_function,
                distance: f32::MIN, // draw grass first
            });
        }
    }
}

pub type DrawGrass = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshBindGroup<1>,
    DrawGrassCommand,
);

pub struct DrawGrassCommand;

impl EntityRenderCommand for DrawGrassCommand {
    type Param = SRes<GrassData>;

    #[inline]
    fn render<'w>(
        _view: Entity,
        _item: Entity,
        query: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let data = query.into_inner();
        pass.set_vertex_buffer(0, data.vertex_buffer.slice(..));
        pass.set_index_buffer(data.index_buffer.slice(..), 0, wgpu::IndexFormat::Uint32);
        pass.draw_indexed_indirect(&data.indirect_buffer, 0);
        RenderCommandResult::Success
    }
}

bitflags::bitflags! {
    #[repr(transparent)]
    pub struct GrassPipelineKey: u32 {
        const NONE = 0;
        const MSAA_RESERVED_BITS = Self::MSAA_MASK_BITS << Self::MSAA_SHIFT_BITS;
    }
}

impl GrassPipelineKey {
    const MSAA_MASK_BITS: u32 = 0b111111;
    const MSAA_SHIFT_BITS: u32 = 32 - 6;

    pub fn from_msaa_samples(msaa_samples: u32) -> Self {
        let msaa_bits = ((msaa_samples - 1) & Self::MSAA_MASK_BITS) << Self::MSAA_SHIFT_BITS;
        Self::from_bits(msaa_bits).unwrap()
    }

    pub fn msaa_samples(&self) -> u32 {
        ((self.bits >> Self::MSAA_SHIFT_BITS) & Self::MSAA_MASK_BITS) + 1
    }
}

pub struct GrassRenderPipeline {
    pub view_layout: BindGroupLayout,
    pub mesh_layout: BindGroupLayout,
    pub shader: Handle<Shader>,
}

impl FromWorld for GrassRenderPipeline {
    fn from_world(world: &mut World) -> Self {
        //let device = world.resource::<RenderDevice>();
        let asset_server = world.resource::<AssetServer>();
        let mesh_pipeline = world.resource::<MeshPipeline>();

        let view_layout = mesh_pipeline.view_layout.clone();
        let mesh_layout = mesh_pipeline.mesh_layout.clone();
        let shader = asset_server.load("shaders/grass_render.wgsl");

        Self {
            view_layout,
            mesh_layout,
            //terrain_layouts: Vec::new(),
            //terrain_view_layout,
            shader,
        }
    }
}

impl SpecializedRenderPipeline for GrassRenderPipeline {
    type Key = GrassPipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        //let shader_defs = key.shader_defs();
        let shader_defs = vec![String::from("VERTEX_UVS")];

        /*
        let vb_desc = VertexBufferLayout {
            array_stride: (std::mem::size_of::<super::DstVertex>()) as wgpu::BufferAddress,
            step_mode: ,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2],
        };
        */
        let vb_desc = VertexBufferLayout::from_vertex_formats(
            wgpu::VertexStepMode::Vertex,
            [
                wgpu::VertexFormat::Float32x3,
                wgpu::VertexFormat::Float32x3,
                wgpu::VertexFormat::Float32x2,
            ],
        );

        RenderPipelineDescriptor {
            label: None,
            layout: Some(vec![
                self.view_layout.clone(),
                //self.terrain_view_layout.clone(),
                //self.terrain_layouts[0].clone(), // Todo: do this properly for multiple maps
                self.mesh_layout.clone(),
            ]),
            vertex: VertexState {
                shader: self.shader.clone(),
                entry_point: "vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![vb_desc],
            },
            primitive: PrimitiveState {
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
                topology: PrimitiveTopology::TriangleStrip,
                strip_index_format: Some(wgpu::IndexFormat::Uint32),
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs,
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::bevy_default(),
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: CompareFunction::Greater,
                stencil: StencilState {
                    front: StencilFaceState::IGNORE,
                    back: StencilFaceState::IGNORE,
                    read_mask: 0,
                    write_mask: 0,
                },
                bias: DepthBiasState {
                    constant: 0,
                    slope_scale: 0.0,
                    clamp: 0.0,
                },
            }),
            multisample: MultisampleState {
                count: key.msaa_samples(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        }
    }
}
