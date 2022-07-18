use bevy::ecs::system::lifetimeless::{Read, SQuery, SRes};
use bevy::ecs::system::SystemParamItem;
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_phase::{
    AddRenderCommand, DrawFunctions, EntityRenderCommand, RenderCommandResult, RenderPhase,
    SetItemPipeline, TrackedRenderPass,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::texture::BevyDefault;
use bevy::render::view::{ExtractedView, ViewDepthTexture};
use bevy::render::{render_resource::*, RenderApp, RenderStage};
use bevy::utils::FloatOrd;

use super::normal_pass::ViewNormalTexture;
use super::postprocess::Postprocess3d;

pub type DrawOutline = (
    SetItemPipeline,
    SetOutlineBindGroup<0>,
    DrawFullscreenTriangle,
);

pub struct OutlinePlugin;

impl Plugin for OutlinePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(ExtractComponentPlugin::<Outline>::default());

        let render_app = match app.get_sub_app_mut(RenderApp) {
            Ok(render_app) => render_app,
            Err(_) => return,
        };

        render_app
            .add_render_command::<Postprocess3d, DrawOutline>()
            .init_resource::<OutlinePipeline>()
            .init_resource::<SpecializedRenderPipelines<OutlinePipeline>>()
            .add_system_to_stage(RenderStage::Prepare, prepare_config)
            .add_system_to_stage(RenderStage::Queue, queue_bind_group)
            .add_system_to_stage(RenderStage::Queue, queue_outline.after(queue_bind_group));
    }
}

pub fn queue_outline(
    specialize_pipeline: Res<OutlinePipeline>,
    draw_functions: Res<DrawFunctions<Postprocess3d>>,
    msaa: Res<Msaa>,
    mut pipelines: ResMut<SpecializedRenderPipelines<OutlinePipeline>>,
    mut pipeline_cache: ResMut<PipelineCache>,
    mut view_query: Query<&mut RenderPhase<Postprocess3d>>,
    query: Query<Entity, With<Outline>>,
) {
    let draw_function = draw_functions.read().get_id::<DrawOutline>().unwrap();

    for mut phase in view_query.iter_mut() {
        for entity in query.iter() {
            let key = OutlinePipelineKey::from_msaa_samples(msaa.samples);
            let pipeline = pipelines.specialize(&mut pipeline_cache, &specialize_pipeline, key);

            phase.add(Postprocess3d {
                entity,
                pipeline,
                draw_function,
                distance: f32::MIN,
            });
        }
    }
}

#[derive(Clone, Component)]
pub struct Outline {
    pub color: Color,

    /// Number of pixels between samples that are tested for an edge.
    /// When this value is 1, tested samples are adjacent.
    pub scale: i32,

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

impl Default for Outline {
    fn default() -> Self {
        Self {
            color: Color::Rgba {
                red: 0.02,
                green: 0.00,
                blue: 0.00,
                alpha: 0.25,
            },
            scale: 0,
            depth_threshold: 1.5,
            depth_normal_threshold: 0.5,
            depth_normal_threshold_scale: 7.0,
            normal_threshold: 0.4,
        }
    }
}

impl ExtractComponent for Outline {
    type Query = Read<Self>;

    type Filter = ();

    fn extract_component(this: bevy::ecs::query::QueryItem<Self::Query>) -> Self {
        this.clone()
    }
}

#[derive(Component)]
pub struct OutlineBuffer(Buffer);

fn prepare_config(
    mut commands: Commands,
    query: Query<(Entity, &ExtractedView, &Outline)>,
    device: Res<RenderDevice>,
) {
    for (entity, view, config) in query.iter() {
        let data = OutlineParams {
            view_space_directon: view.transform.mul_vec3(Vec3::Z).extend(0.0).into(),
            color: config.color.as_rgba_f32(),
            scale: config.scale,
            depth_threshold: config.depth_threshold,
            depth_normal_threshold: config.depth_normal_threshold,
            depth_normal_threshold_scale: config.depth_normal_threshold_scale,
            normal_threshold: config.normal_threshold,
            pad: [0.0; 3],
        };

        let buffer = device.create_buffer_with_data(&wgpu::util::BufferInitDescriptor {
            label: Some("outline params"),
            contents: bytemuck::bytes_of(&data),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        commands.entity(entity).insert(OutlineBuffer(buffer));
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct OutlineParams {
    view_space_directon: [f32; 4],
    color: [f32; 4],
    scale: i32,
    pad: [f32; 3],
    depth_threshold: f32, // 0..1
    depth_normal_threshold: f32,
    depth_normal_threshold_scale: f32,
    normal_threshold: f32, // 0..1
}

#[derive(Component)]
pub struct OutlineBindGroup(BindGroup);

fn queue_bind_group(
    mut commands: Commands,
    pipeline: Res<OutlinePipeline>,
    device: Res<RenderDevice>,
    query: Query<(
        Entity,
        &OutlineBuffer,
        &ViewDepthTexture,
        &ViewNormalTexture,
    )>,
) {
    for (entity, buffer, depth, normal) in query.iter() {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("outline"),
            layout: &pipeline.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.0.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&depth.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::TextureView(&normal.view),
                },
            ],
        });

        let component = OutlineBindGroup(bind_group);
        commands.entity(entity).insert(component);
    }
}

pub struct OutlinePipeline {
    bind_group_layout: BindGroupLayout,
    shader: Handle<Shader>,
}

impl FromWorld for OutlinePipeline {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();
        let asset_server = world.resource::<AssetServer>();

        //let multisampled = sample_count > 1;
        let multisampled = false;
        let uniform_size = std::mem::size_of::<OutlineParams>() as wgpu::BufferAddress;

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("outline"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
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
        });

        let shader = asset_server.load("shaders/outline.wgsl");

        Self {
            bind_group_layout,
            shader,
        }
    }
}

bitflags::bitflags! {
    #[repr(transparent)]
    pub struct OutlinePipelineKey: u32 {
        const NONE = 0;
        const MSAA_RESERVED_BITS = Self::MSAA_MASK_BITS << Self::MSAA_SHIFT_BITS;
    }
}

impl OutlinePipelineKey {
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

impl SpecializedRenderPipeline for OutlinePipeline {
    type Key = OutlinePipelineKey;

    fn specialize(&self, key: Self::Key) -> RenderPipelineDescriptor {
        //let shader_defs = key.shader_defs();
        let shader_defs = vec![];
        //let shader_defs = vec![String::from("VERTEX_UVS")];

        RenderPipelineDescriptor {
            label: None,
            layout: Some(vec![self.bind_group_layout.clone()]),
            vertex: VertexState {
                shader: self.shader.clone(),
                entry_point: "vertex".into(),
                shader_defs: shader_defs.clone(),
                buffers: vec![],
            },
            primitive: PrimitiveState {
                front_face: FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
                topology: PrimitiveTopology::TriangleList,
                strip_index_format: None,
            },
            fragment: Some(FragmentState {
                shader: self.shader.clone(),
                shader_defs,
                entry_point: "fragment".into(),
                targets: vec![Some(ColorTargetState {
                    format: TextureFormat::bevy_default(),
                    blend: Some(BlendState::ALPHA_BLENDING),
                    //blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
            }),
            depth_stencil: None,
            multisample: MultisampleState {
                count: key.msaa_samples(),
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
        }
    }
}

pub struct DrawFullscreenTriangle;

impl EntityRenderCommand for DrawFullscreenTriangle {
    type Param = ();

    #[inline]
    fn render<'w>(
        _view: Entity,
        _item: Entity,
        _query: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        pass.draw(0..3, 0..1);
        RenderCommandResult::Success
    }
}

pub struct SetOutlineBindGroup<const I: usize>;
impl<const I: usize> EntityRenderCommand for SetOutlineBindGroup<I> {
    type Param = SQuery<Read<OutlineBindGroup>>;

    #[inline]
    fn render<'w>(
        _view: Entity,
        item: Entity,
        query: SystemParamItem<'w, '_, Self::Param>,
        pass: &mut TrackedRenderPass<'w>,
    ) -> RenderCommandResult {
        let bind_group = query.get_inner(item).unwrap();
        pass.set_bind_group(I, &bind_group.0, &[]);
        RenderCommandResult::Success
    }
}
