use bevy::pbr::{DrawMesh, MeshUniform, SetMeshBindGroup, SetMeshViewBindGroup};
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponentPlugin;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{RenderGraph, RunGraphOnViewNode};
use bevy::render::render_phase::{
    sort_phase_system, AddRenderCommand, SetItemPipeline, TrackedRenderPass,
};
use bevy::render::{
    camera::ExtractedCamera, render_phase::RenderPhase, render_resource::*, renderer::RenderDevice,
    texture::TextureCache, Extract, RenderApp, RenderStage,
};
use bevy::utils::HashMap;

pub mod draw_normal_graph {
    pub const NAME: &str = "draw_normal";

    pub mod node {
        /// Label for the normal pass node.
        pub const NORMAL_PASS: &str = "normal_pass";
    }
}

pub type DrawNormalMesh = (
    SetItemPipeline,
    SetMeshViewBindGroup<0>,
    SetMeshBindGroup<1>,
    DrawMesh,
);

pub struct NormalPassPlugin;

impl Plugin for NormalPassPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(ExtractComponentPlugin::<NormalPassMaterial>::default());

        let render_app = match app.get_sub_app_mut(RenderApp) {
            Ok(render_app) => render_app,
            Err(_) => return,
        };

        render_app
            .init_resource::<DrawFunctions<Normal3d>>()
            .add_render_command::<Normal3d, DrawNormalMesh>()
            .init_resource::<NormalPassPipeline>()
            .init_resource::<SpecializedMeshPipelines<NormalPassPipeline>>()
            .add_system_to_stage(RenderStage::Extract, extract_normal_3d_camera_phases)
            .add_system_to_stage(RenderStage::Prepare, prepare_core_3d_normal_textures)
            .add_system_to_stage(RenderStage::Queue, queue_normal_material)
            .add_system_to_stage(RenderStage::PhaseSort, sort_phase_system::<Normal3d>);

        let normal_pass_node = NormalPassNode::new(&mut render_app.world);
        let mut graph = render_app.world.resource_mut::<RenderGraph>();

        let draw_3d_graph = graph
            .get_sub_graph_mut(bevy::core_pipeline::core_3d::graph::NAME)
            .unwrap();
        draw_3d_graph.add_node(draw_normal_graph::node::NORMAL_PASS, normal_pass_node);

        draw_3d_graph
            .add_node_edge(
                bevy::core_pipeline::core_3d::graph::node::MAIN_PASS,
                draw_normal_graph::node::NORMAL_PASS,
            )
            .unwrap();

        draw_3d_graph
            .add_slot_edge(
                draw_3d_graph.input_node().unwrap().id,
                bevy::core_pipeline::core_3d::graph::input::VIEW_ENTITY,
                draw_normal_graph::node::NORMAL_PASS,
                NormalPassNode::IN_VIEW,
            )
            .unwrap();
    }
}

pub fn extract_normal_3d_camera_phases(
    mut commands: Commands,
    cameras_3d: Extract<Query<(Entity, &Camera), With<Camera3d>>>,
) {
    for (entity, camera) in cameras_3d.iter() {
        if camera.is_active {
            commands
                .get_or_spawn(entity)
                .insert(RenderPhase::<Normal3d>::default());
        }
    }
}

#[derive(Component)]
pub struct ViewNormalTexture {
    pub texture: Texture,
    pub view: TextureView,
}

impl ViewNormalTexture {
    pub fn get_color_attachment(&self, ops: Operations<wgpu::Color>) -> RenderPassColorAttachment {
        RenderPassColorAttachment {
            view: &self.view,
            resolve_target: None,
            ops,
        }
    }
}

pub fn prepare_core_3d_normal_textures(
    mut commands: Commands,
    mut texture_cache: ResMut<TextureCache>,
    msaa: Res<Msaa>,
    render_device: Res<RenderDevice>,
    views_3d: Query<(Entity, &ExtractedCamera), With<RenderPhase<Normal3d>>>,
) {
    let mut textures = HashMap::default();
    for (entity, camera) in &views_3d {
        if let Some(physical_target_size) = camera.physical_target_size {
            let cached_texture = textures
                .entry(camera.target.clone())
                .or_insert_with(|| {
                    texture_cache.get(
                        &render_device,
                        TextureDescriptor {
                            label: Some("view_normal_texture"),
                            size: Extent3d {
                                depth_or_array_layers: 1,
                                width: physical_target_size.x,
                                height: physical_target_size.y,
                            },
                            mip_level_count: 1,
                            sample_count: msaa.samples,
                            dimension: TextureDimension::D2,
                            format: TextureFormat::Rgb10a2Unorm,
                            usage: TextureUsages::RENDER_ATTACHMENT
                                | TextureUsages::TEXTURE_BINDING,
                        },
                    )
                })
                .clone();

            commands.entity(entity).insert(ViewNormalTexture {
                texture: cached_texture.texture,
                view: cached_texture.default_view,
            });
        }
    }
}

// ---------------------------------------------

use bevy::ecs::system::lifetimeless::Read;
use bevy::pbr::{MeshPipeline, MeshPipelineKey};
use bevy::render::extract_component::ExtractComponent;
use bevy::render::mesh::MeshVertexBufferLayout;

#[derive(Component)]
pub struct NormalPassMaterial;

impl ExtractComponent for NormalPassMaterial {
    type Query = Read<Self>;

    type Filter = ();

    fn extract_component(_: bevy::ecs::query::QueryItem<Self::Query>) -> Self {
        Self
    }
}

pub struct NormalPassPipeline {
    shader: Handle<Shader>,
    mesh_pipeline: MeshPipeline,
}

impl FromWorld for NormalPassPipeline {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        Self {
            shader: asset_server.load("shaders/normal_pass.wgsl"),
            mesh_pipeline: world.resource::<MeshPipeline>().clone(),
        }
    }
}

impl SpecializedMeshPipeline for NormalPassPipeline {
    type Key = MeshPipelineKey;

    fn specialize(
        &self,
        key: Self::Key,
        layout: &MeshVertexBufferLayout,
    ) -> Result<RenderPipelineDescriptor, SpecializedMeshPipelineError> {
        let mut descriptor = self.mesh_pipeline.specialize(key, layout)?;
        descriptor.label = Some("normal pass".into());
        descriptor.vertex.shader = self.shader.clone();
        let frag = descriptor.fragment.as_mut().unwrap();
        frag.shader = self.shader.clone();

        //let blend = frag.targets[0].as_ref().unwrap().blend;
        let blend = Some(BlendState::REPLACE);
        frag.targets = vec![Some(ColorTargetState {
            format: TextureFormat::Rgb10a2Unorm,
            blend,
            write_mask: ColorWrites::ALL,
        })];

        descriptor.layout = Some(vec![
            self.mesh_pipeline.view_layout.clone(),
            self.mesh_pipeline.mesh_layout.clone(),
        ]);
        descriptor.depth_stencil = Some(DepthStencilState {
            format: TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: CompareFunction::GreaterEqual,
            stencil: StencilState::default(),
            bias: DepthBiasState::default(),
        });

        Ok(descriptor)
    }
}

pub fn queue_normal_material(
    draw_functions: Res<DrawFunctions<Normal3d>>,
    specialize_pipeline: Res<NormalPassPipeline>,
    msaa: Res<Msaa>,
    mut pipelines: ResMut<SpecializedMeshPipelines<NormalPassPipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
    render_meshes: Res<RenderAssets<Mesh>>,
    material_meshes: Query<(Entity, &MeshUniform, &Handle<Mesh>), With<NormalPassMaterial>>,
    mut views: Query<(&ExtractedView, &mut RenderPhase<Normal3d>)>,
) {
    let draw_function = draw_functions.read().get_id::<DrawNormalMesh>().unwrap();

    let key = MeshPipelineKey::from_msaa_samples(msaa.samples)
        | MeshPipelineKey::from_primitive_topology(PrimitiveTopology::TriangleList);

    for (view, mut phase) in &mut views {
        let rangefinder = view.rangefinder3d();
        for (entity, mesh_uniform, mesh_handle) in &material_meshes {
            if let Some(mesh) = render_meshes.get(mesh_handle) {
                let pipeline = pipelines
                    .specialize(&mut pipeline_cache, &specialize_pipeline, key, &mesh.layout)
                    .unwrap();
                phase.add(Normal3d {
                    entity,
                    pipeline,
                    draw_function,
                    distance: rangefinder.distance(&mesh_uniform.transform),
                });
            }
        }
    }
}

// ---------------------------------------------

use bevy::render::render_graph::{Node, NodeRunError, RenderGraphContext, SlotInfo, SlotType};
use bevy::render::render_phase::DrawFunctions;
use bevy::render::renderer::RenderContext;
use bevy::render::view::ExtractedView;
use bevy::render::view::ViewDepthTexture;

pub struct NormalPassNode {
    query: QueryState<
        (
            Read<ExtractedCamera>,
            Read<RenderPhase<Normal3d>>,
            Read<ViewDepthTexture>,
            Read<ViewNormalTexture>,
        ),
        With<ExtractedView>,
    >,
}

impl NormalPassNode {
    pub const IN_VIEW: &'static str = "view";

    pub fn new(world: &mut World) -> Self {
        Self {
            query: world.query_filtered(),
        }
    }
}

impl Node for NormalPassNode {
    fn input(&self) -> Vec<SlotInfo> {
        vec![SlotInfo::new(Self::IN_VIEW, SlotType::Entity)]
    }

    fn update(&mut self, world: &mut World) {
        self.query.update_archetypes(world);
    }

    fn run(
        &self,
        graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let view_entity = graph.get_input_entity(Self::IN_VIEW)?;
        let (camera, phase, depth, normal) = match self.query.get_manual(world, view_entity) {
            Ok(query) => query,
            Err(_) => return Ok(()), // No window
        };

        // Always run normal pass to ensure normal texture is cleared
        {
            #[cfg(feature = "trace")]
            let _span = info_span!("normal_pass_3d").entered();
            let pass_descriptor = RenderPassDescriptor {
                label: Some("normal_pass_3d"),
                color_attachments: &[Some(normal.get_color_attachment(Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: true,
                }))],
                depth_stencil_attachment: Some(RenderPassDepthStencilAttachment {
                    view: &depth.view,
                    // NOTE: The normal pass only loads the depth buffer
                    depth_ops: Some(Operations {
                        // NOTE: 0.0 is the far plane due to bevy's use of reverse-z projections.
                        load: wgpu::LoadOp::Load,
                        store: false,
                    }),
                    stencil_ops: None,
                }),
            };

            let draw_functions = world.resource::<DrawFunctions<Normal3d>>();

            let render_pass = render_context
                .command_encoder
                .begin_render_pass(&pass_descriptor);
            let mut draw_functions = draw_functions.write();
            let mut tracked_pass = TrackedRenderPass::new(render_pass);
            if let Some(viewport) = camera.viewport.as_ref() {
                tracked_pass.set_camera_viewport(viewport);
            }
            for item in &phase.items {
                let draw_function = draw_functions.get_mut(item.draw_function).unwrap();
                draw_function.draw(world, &mut tracked_pass, view_entity, item);
            }
        }

        Ok(())
    }
}

// ---------------------------------------------

use bevy::render::{
    render_phase::{CachedRenderPipelinePhaseItem, DrawFunctionId, EntityPhaseItem, PhaseItem},
    render_resource::CachedRenderPipelineId,
};
use bevy::utils::FloatOrd;
use std::cmp::Reverse;

pub struct Normal3d {
    pub distance: f32,
    pub pipeline: CachedRenderPipelineId,
    pub entity: Entity,
    pub draw_function: DrawFunctionId,
}

impl PhaseItem for Normal3d {
    // NOTE: Values increase towards the camera. Front-to-back ordering for opaque means we need a descending sort.
    type SortKey = Reverse<FloatOrd>;

    #[inline]
    fn sort_key(&self) -> Self::SortKey {
        Reverse(FloatOrd(self.distance))
    }

    #[inline]
    fn draw_function(&self) -> DrawFunctionId {
        self.draw_function
    }

    #[inline]
    fn sort(items: &mut [Self]) {
        // Key negated to match reversed SortKey ordering
        radsort::sort_by_key(items, |item| -item.distance);
    }
}

impl EntityPhaseItem for Normal3d {
    #[inline]
    fn entity(&self) -> Entity {
        self.entity
    }
}

impl CachedRenderPipelinePhaseItem for Normal3d {
    #[inline]
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.pipeline
    }
}
