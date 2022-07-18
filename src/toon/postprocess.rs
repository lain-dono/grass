use bevy::ecs::system::lifetimeless::Read;
use bevy::pbr::{DrawMesh, MeshUniform, SetMeshBindGroup, SetMeshViewBindGroup};
use bevy::prelude::*;
use bevy::render::extract_component::ExtractComponentPlugin;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_graph::{Node, NodeRunError, RenderGraphContext, SlotInfo, SlotType};
use bevy::render::render_graph::{RenderGraph, RunGraphOnViewNode};
use bevy::render::render_phase::{sort_phase_system, AddRenderCommand, SetItemPipeline};
use bevy::render::render_phase::{DrawFunctions, TrackedRenderPass};
use bevy::render::renderer::RenderContext;
use bevy::render::view::ViewDepthTexture;
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{
    camera::ExtractedCamera, render_phase::RenderPhase, render_resource::*, renderer::RenderDevice,
    texture::TextureCache, Extract, RenderApp, RenderStage,
};
use bevy::render::{
    render_phase::{CachedRenderPipelinePhaseItem, DrawFunctionId, EntityPhaseItem, PhaseItem},
    render_resource::CachedRenderPipelineId,
};
use bevy::utils::FloatOrd;
use bevy::utils::HashMap;
use std::cmp::Reverse;

pub mod draw_postprocess_graph {
    pub const NAME: &str = "draw_postprocess";

    pub mod node {
        /// Label for the postprocess pass node.
        pub const POSTPROCESS_PASS: &str = "postprocess_pass";
    }
}

pub struct PostprocessPassPlugin;

impl Plugin for PostprocessPassPlugin {
    fn build(&self, app: &mut App) {
        let render_app = match app.get_sub_app_mut(RenderApp) {
            Ok(render_app) => render_app,
            Err(_) => return,
        };

        render_app
            .init_resource::<DrawFunctions<Postprocess3d>>()
            .add_system_to_stage(RenderStage::Extract, extract_postprocess_3d_camera_phases)
            .add_system_to_stage(RenderStage::PhaseSort, sort_phase_system::<Postprocess3d>);

        /*
        let postprocess_pass_node = PostprocessPassNode::new(&mut render_app.world);

        let mut graph = render_app.world.resource_mut::<RenderGraph>();
        let draw_3d_graph = graph
            .get_sub_graph_mut(bevy::core_pipeline::core_3d::graph::NAME)
            .unwrap();
        draw_3d_graph.add_node(
            draw_postprocess_graph::node::POSTPROCESS_PASS,
            postprocess_pass_node,
        );

        draw_3d_graph
            .add_node_edge(
                bevy::core_pipeline::core_3d::graph::node::MAIN_PASS,
                draw_postprocess_graph::node::POSTPROCESS_PASS,
            )
            .unwrap();

        draw_3d_graph
            .add_node_edge(
                super::normal_pass::draw_normal_graph::node::NORMAL_PASS,
                draw_postprocess_graph::node::POSTPROCESS_PASS,
            )
            .unwrap();

        draw_3d_graph
            .add_slot_edge(
                draw_3d_graph.input_node().unwrap().id,
                bevy::core_pipeline::core_3d::graph::input::VIEW_ENTITY,
                draw_postprocess_graph::node::POSTPROCESS_PASS,
                PostprocessPassNode::IN_VIEW,
            )
            .unwrap();
        */

        let sub_graph = get_graph(render_app);
        let mut graph = render_app.world.resource_mut::<RenderGraph>();
        if let Some(graph_3d) = graph.get_sub_graph_mut(bevy::core_pipeline::core_3d::graph::NAME) {
            graph_3d.add_sub_graph(draw_postprocess_graph::NAME, sub_graph);
            graph_3d.add_node(
                draw_postprocess_graph::node::POSTPROCESS_PASS,
                RunGraphOnViewNode::new(draw_postprocess_graph::NAME),
            );
            graph_3d
                .add_node_edge(
                    bevy::core_pipeline::core_3d::graph::node::MAIN_PASS,
                    draw_postprocess_graph::node::POSTPROCESS_PASS,
                )
                .unwrap();
            graph_3d
                .add_slot_edge(
                    graph_3d.input_node().unwrap().id,
                    bevy::core_pipeline::core_3d::graph::input::VIEW_ENTITY,
                    draw_postprocess_graph::node::POSTPROCESS_PASS,
                    RunGraphOnViewNode::IN_VIEW,
                )
                .unwrap();
        }
    }
}

fn get_graph(render_app: &mut App) -> RenderGraph {
    let node = PostprocessPassNode::new(&mut render_app.world);
    let mut graph = RenderGraph::default();
    graph.add_node(draw_postprocess_graph::node::POSTPROCESS_PASS, node);
    let input_node_id = graph.set_input(vec![SlotInfo::new(
        bevy::core_pipeline::core_3d::graph::input::VIEW_ENTITY,
        SlotType::Entity,
    )]);
    graph
        .add_slot_edge(
            input_node_id,
            bevy::core_pipeline::core_3d::graph::input::VIEW_ENTITY,
            draw_postprocess_graph::node::POSTPROCESS_PASS,
            PostprocessPassNode::IN_VIEW,
        )
        .unwrap();
    graph
}

pub fn extract_postprocess_3d_camera_phases(
    mut commands: Commands,
    cameras_3d: Extract<Query<(Entity, &Camera), With<Camera3d>>>,
) {
    for (entity, camera) in cameras_3d.iter() {
        if camera.is_active {
            commands
                .get_or_spawn(entity)
                .insert(RenderPhase::<Postprocess3d>::default());
        }
    }
}

pub struct PostprocessPassNode {
    query: QueryState<
        (
            Read<ExtractedCamera>,
            Read<RenderPhase<Postprocess3d>>,
            Read<ViewTarget>,
        ),
        With<ExtractedView>,
    >,
}

impl PostprocessPassNode {
    pub const IN_VIEW: &'static str = "view";

    pub fn new(world: &mut World) -> Self {
        Self {
            query: world.query_filtered(),
        }
    }
}

impl Node for PostprocessPassNode {
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
        let (camera, phase, target) = match self.query.get_manual(world, view_entity) {
            Ok(query) => query,
            Err(_) => return Ok(()), // No window
        };

        if !phase.items.is_empty() {
            #[cfg(feature = "trace")]
            let _span = info_span!("postprocess_pass_3d").entered();
            let pass_descriptor = RenderPassDescriptor {
                label: Some("postprocess_pass_3d"),
                color_attachments: &[Some(target.get_color_attachment(Operations {
                    load: wgpu::LoadOp::Load,
                    store: true,
                }))],
                depth_stencil_attachment: None,
            };

            let draw_functions = world.resource::<DrawFunctions<Postprocess3d>>();

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

pub struct Postprocess3d {
    pub distance: f32,
    pub pipeline: CachedRenderPipelineId,
    pub entity: Entity,
    pub draw_function: DrawFunctionId,
}

impl PhaseItem for Postprocess3d {
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

impl EntityPhaseItem for Postprocess3d {
    #[inline]
    fn entity(&self) -> Entity {
        self.entity
    }
}

impl CachedRenderPipelinePhaseItem for Postprocess3d {
    #[inline]
    fn cached_pipeline(&self) -> CachedRenderPipelineId {
        self.pipeline
    }
}
