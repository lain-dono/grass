use bevy::core_pipeline::clear_color::ClearColor;
use bevy::core_pipeline::core_2d::Core2dPlugin;
use bevy::core_pipeline::core_3d::{
    extract_core_3d_camera_phases, graph, AlphaMask3d, Camera3d, MainPass3dNode, Opaque3d,
    Transparent3d,
};
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::render_phase::RenderPhase;
use bevy::render::renderer::RenderDevice;
use bevy::render::texture::TextureCache;
use bevy::render::view::ViewDepthTexture;
use bevy::render::{
    extract_component::ExtractComponentPlugin,
    extract_resource::ExtractResourcePlugin,
    render_graph::{RenderGraph, SlotInfo, SlotType},
    render_phase::{sort_phase_system, DrawFunctions},
    RenderApp, RenderStage,
};
use bevy::utils::HashMap;
use wgpu::{Extent3d, TextureDescriptor, TextureDimension, TextureFormat, TextureUsages};

#[derive(Default)]
pub struct CorePipelinePlugin;

impl Plugin for CorePipelinePlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<ClearColor>()
            .init_resource::<ClearColor>()
            .add_plugin(ExtractResourcePlugin::<ClearColor>::default())
            .add_plugin(Core2dPlugin)
            .add_plugin(Core3dPlugin);
    }
}

pub struct Core3dPlugin;

impl Plugin for Core3dPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<Camera3d>()
            .add_plugin(ExtractComponentPlugin::<Camera3d>::default());

        let render_app = match app.get_sub_app_mut(RenderApp) {
            Ok(render_app) => render_app,
            Err(_) => return,
        };

        render_app
            .init_resource::<DrawFunctions<Opaque3d>>()
            .init_resource::<DrawFunctions<AlphaMask3d>>()
            .init_resource::<DrawFunctions<Transparent3d>>()
            .add_system_to_stage(RenderStage::Extract, extract_core_3d_camera_phases)
            .add_system_to_stage(RenderStage::Prepare, self::prepare_core_3d_depth_textures)
            .add_system_to_stage(RenderStage::PhaseSort, sort_phase_system::<Opaque3d>)
            .add_system_to_stage(RenderStage::PhaseSort, sort_phase_system::<AlphaMask3d>)
            .add_system_to_stage(RenderStage::PhaseSort, sort_phase_system::<Transparent3d>);

        let pass_node_3d = MainPass3dNode::new(&mut render_app.world);
        let mut graph = render_app.world.resource_mut::<RenderGraph>();

        let mut draw_3d_graph = RenderGraph::default();
        draw_3d_graph.add_node(graph::node::MAIN_PASS, pass_node_3d);
        let input_node_id = draw_3d_graph.set_input(vec![SlotInfo::new(
            graph::input::VIEW_ENTITY,
            SlotType::Entity,
        )]);
        draw_3d_graph
            .add_slot_edge(
                input_node_id,
                graph::input::VIEW_ENTITY,
                graph::node::MAIN_PASS,
                MainPass3dNode::IN_VIEW,
            )
            .unwrap();
        graph.add_sub_graph(graph::NAME, draw_3d_graph);
    }
}

pub fn prepare_core_3d_depth_textures(
    mut commands: Commands,
    mut texture_cache: ResMut<TextureCache>,
    msaa: Res<Msaa>,
    render_device: Res<RenderDevice>,
    views_3d: Query<
        (Entity, &ExtractedCamera),
        (
            With<RenderPhase<Opaque3d>>,
            With<RenderPhase<AlphaMask3d>>,
            With<RenderPhase<Transparent3d>>,
        ),
    >,
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
                            label: Some("view_depth_texture"),
                            size: Extent3d {
                                depth_or_array_layers: 1,
                                width: physical_target_size.x,
                                height: physical_target_size.y,
                            },
                            mip_level_count: 1,
                            sample_count: msaa.samples,
                            dimension: TextureDimension::D2,
                            format: TextureFormat::Depth32Float, /* PERF: vulkan docs recommend using 24
                                                                  * bit depth for better performance */
                            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
                        },
                    )
                })
                .clone();
            commands.entity(entity).insert(ViewDepthTexture {
                texture: cached_texture.texture,
                view: cached_texture.default_view,
            });
        }
    }
}
