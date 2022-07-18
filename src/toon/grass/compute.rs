use bevy::{
    prelude::*,
    render::{
        render_graph,
        render_resource::*,
        renderer::{RenderContext, RenderDevice},
    },
};
use std::borrow::Cow;
use std::mem::size_of;

use super::{DrawIndexedIndirect, GrassBindGroup, GrassUniform};

pub struct GrassComputePipeline {
    pub compute_bind_group_layout: BindGroupLayout,
    pub init_pipeline: CachedComputePipelineId,
    pub fill_pipeline: CachedComputePipelineId,
}

impl FromWorld for GrassComputePipeline {
    fn from_world(world: &mut World) -> Self {
        let device = world.resource::<RenderDevice>();

        let compute_bind_group_layout = device.create_bind_group_layout(&COMPUTE_LAYOUT);

        let compute_shader = world
            .resource::<AssetServer>()
            .load("shaders/grass_compute.wgsl");

        let mut pipeline_cache = world.resource_mut::<PipelineCache>();
        let init_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: None,
            layout: Some(vec![compute_bind_group_layout.clone()]),
            shader: compute_shader.clone(),
            shader_defs: vec![],
            entry_point: Cow::from("cs_main_init"),
        });
        let fill_pipeline = pipeline_cache.queue_compute_pipeline(ComputePipelineDescriptor {
            label: None,
            layout: Some(vec![compute_bind_group_layout.clone()]),
            shader: compute_shader,
            shader_defs: vec![],
            entry_point: Cow::from("cs_main_fill"),
        });

        Self {
            compute_bind_group_layout,
            init_pipeline,
            fill_pipeline,
        }
    }
}

#[derive(Default)]
pub struct GrassComputeNode {}

impl render_graph::Node for GrassComputeNode {
    fn run(
        &self,
        _graph: &mut render_graph::RenderGraphContext,
        context: &mut RenderContext,
        world: &World,
    ) -> Result<(), render_graph::NodeRunError> {
        let GrassBindGroup { bind_group, count } = &world.resource::<GrassBindGroup>();
        let pipeline_cache = world.resource::<PipelineCache>();
        let pipeline = world.resource::<GrassComputePipeline>();

        let init_pipeline = pipeline_cache.get_compute_pipeline(pipeline.init_pipeline);
        let fill_pipeline = pipeline_cache.get_compute_pipeline(pipeline.fill_pipeline);
        let (init_pipeline, fill_pipeline) = match (init_pipeline, fill_pipeline) {
            (Some(init), Some(fill)) => (init, fill),
            _ => return Ok(()),
        };

        let mut pass = context.command_encoder.begin_compute_pass(&default());

        pass.set_bind_group(0, bind_group, &[]);

        pass.set_pipeline(init_pipeline);
        pass.dispatch_workgroups(1, 1, 1);
        pass.set_pipeline(fill_pipeline);
        pass.dispatch_workgroups(*count, 1, 1);

        Ok(())
    }
}

const COMPUTE_LAYOUT: wgpu::BindGroupLayoutDescriptor = wgpu::BindGroupLayoutDescriptor {
    label: Some("grass"),
    entries: &[
        wgpu::BindGroupLayoutEntry {
            binding: 0, // params
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(size_of::<GrassUniform>() as u64),
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
                min_binding_size: wgpu::BufferSize::new(size_of::<DrawIndexedIndirect>() as u64),
            },
            count: None,
        },
    ],
};
