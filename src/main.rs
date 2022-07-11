use std::{f32::consts, iter, mem, num::NonZeroU32, ops::Range, rc::Rc};

mod entity;
mod framework;
mod grass;
mod mesh;
mod post;

use self::entity::{EntityPipeline, EntityUniforms, GlobalUniforms, LightRaw};
use self::grass::Grass;
use self::mesh::{create_cube, create_terrain};
use self::post::Postprocess;
use glam::{Mat4, Vec4};
use wgpu::util::{align_to, DeviceExt};

fn main() {
    framework::run::<Example>("shadow");
}

#[inline]
pub fn perspective_wgpu_dx(vertical_fov: f32, aspect_ratio: f32, z_near: f32, z_far: f32) -> Mat4 {
    let t = (vertical_fov / 2.0).tan();
    let sy = 1.0 / t;
    let sx = sy / aspect_ratio;
    let nmf = z_near - z_far;

    Mat4 {
        x_axis: Vec4::new(sx, 0.0, 0.0, 0.0),
        y_axis: Vec4::new(0.0, sy, 0.0, 0.0),
        z_axis: Vec4::new(0.0, 0.0, z_far / nmf, -1.0),
        w_axis: Vec4::new(0.0, 0.0, z_near * z_far / nmf, 0.0),
    }
}

#[inline]
pub fn perspective_infinite_z_wgpu_dx(vertical_fov: f32, aspect_ratio: f32, z_near: f32) -> Mat4 {
    let t = (vertical_fov / 2.0).tan();
    let sy = 1.0 / t;
    let sx = sy / aspect_ratio;

    Mat4 {
        x_axis: Vec4::new(sx, 0.0, 0.0, 0.0),
        y_axis: Vec4::new(0.0, sy, 0.0, 0.0),
        z_axis: Vec4::new(0.0, 0.0, -1.0, -1.0),
        w_axis: Vec4::new(0.0, 0.0, -z_near, 0.0),
    }
}

fn perspective_light(vertical_fov: f32, aspect_ratio: f32, z_near: f32, z_far: f32) -> Mat4 {
    //perspective_wgpu_dx(vertical_fov, aspect_ratio, z_near, z_far)
    perspective_infinite_z_wgpu_dx(vertical_fov, aspect_ratio, z_near)
}

fn perspective_scene(vertical_fov: f32, aspect_ratio: f32, z_near: f32, z_far: f32) -> Mat4 {
    perspective_wgpu_dx(vertical_fov, aspect_ratio, z_near, z_far)
    //perspective_infinite_z_wgpu_dx(vertical_fov, aspect_ratio, z_near)
}

struct Entity {
    mx_world: glam::Mat4,
    rotation_speed: f32,
    color: wgpu::Color,
    vertex_buf: Rc<wgpu::Buffer>,
    index_buf: Rc<wgpu::Buffer>,
    index_format: wgpu::IndexFormat,
    index_count: usize,
    uniform_offset: wgpu::DynamicOffset,
}

struct Light {
    pos: glam::Vec3,
    color: wgpu::Color,
    fov: f32,
    depth: Range<f32>,
    target_view: wgpu::TextureView,
}

impl Light {
    fn to_raw(&self) -> LightRaw {
        let view = glam::Mat4::look_at_rh(self.pos, glam::Vec3::ZERO, glam::Vec3::Y);
        let projection = perspective_light(
            self.fov * consts::PI / 180.,
            1.0,
            self.depth.start,
            self.depth.end,
        );
        let view_proj = projection * view;
        LightRaw {
            proj: view_proj.to_cols_array_2d(),
            pos: [self.pos.x, self.pos.y, self.pos.z, 1.0],
            color: [
                self.color.r as f32,
                self.color.g as f32,
                self.color.b as f32,
                1.0,
            ],
        }
    }
}

struct RenderPass {
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
}

struct Example {
    entities: Vec<Entity>,

    lights: LightManager,

    shadow_pass: RenderPass,
    forward_pass: RenderPass,

    framebuffer: Framebuffer,

    entity_bind_group: wgpu::BindGroup,
    entity_uniform_buf: wgpu::Buffer,

    entity_pipeline: EntityPipeline,
    grass: Grass,

    extra_offset: wgpu::DynamicOffset,

    eye: glam::Vec3,
    show_grass: bool,
    show_postprocess: bool,
    post: Postprocess,
}

impl Example {
    const SHADOW_SIZE: wgpu::Extent3d = wgpu::Extent3d {
        width: 2048,
        height: 2048,
        depth_or_array_layers: EntityPipeline::MAX_LIGHTS as u32,
    };

    fn generate_matrix(aspect_ratio: f32, eye: glam::Vec3) -> glam::Mat4 {
        let projection = perspective_scene(consts::FRAC_PI_2, aspect_ratio, 0.1, 200.0);
        let view = glam::Mat4::look_at_rh(eye, glam::Vec3::new(0f32, 0.0, 0.0), glam::Vec3::Y);
        projection * view
    }
}

impl framework::Example for Example {
    fn optional_features() -> wgpu::Features {
        wgpu::Features::DEPTH_CLIP_CONTROL
    }

    /*
    fn required_features() -> wgpu::Features {
        wgpu::Features::MULTI_DRAW_INDIRECT
    }
    */

    fn required_limits() -> wgpu::Limits {
        wgpu::Limits::downlevel_defaults()
    }

    fn init(
        sc_desc: &wgpu::SurfaceConfiguration,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Self {
        let supports_storage_resources = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::VERTEX_STORAGE)
            && device.limits().max_storage_buffers_per_shader_stage > 0;

        let sample_count = 1;
        let entity_pipeline = EntityPipeline::new(device, sc_desc.format, sample_count);
        let grass = Grass::new(device, &entity_pipeline, sc_desc.format, sample_count);

        // Create the vertex and index buffers
        let (cube_vertex_data, cube_index_data) = create_cube();
        let cube_vertex_buf = Rc::new(device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Cubes Vertex Buffer"),
                contents: bytemuck::cast_slice(&cube_vertex_data),
                usage: wgpu::BufferUsages::VERTEX,
            },
        ));

        let cube_index_buf = Rc::new(device.create_buffer_init(
            &wgpu::util::BufferInitDescriptor {
                label: Some("Cubes Index Buffer"),
                contents: bytemuck::cast_slice(&cube_index_data),
                usage: wgpu::BufferUsages::INDEX,
            },
        ));

        let (plane_vertex_data, plane_index_data) = create_terrain(25, 25);
        let plane_vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Plane Vertex Buffer"),
            contents: bytemuck::cast_slice(&plane_vertex_data),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let plane_index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Plane Index Buffer"),
            contents: bytemuck::cast_slice(&plane_index_data),
            usage: wgpu::BufferUsages::INDEX,
        });

        struct CubeDesc {
            offset: glam::Vec3,
            angle: f32,
            scale: f32,
            rotation: f32,
        }
        let cube_descs = [
            CubeDesc {
                offset: glam::Vec3::new(-2.0, 2.0, -2.0),
                angle: 10.0,
                scale: 0.7,
                rotation: 0.1,
            },
            CubeDesc {
                offset: glam::Vec3::new(2.0, 2.0, -2.0),
                angle: 50.0,
                scale: 1.3,
                rotation: 0.2,
            },
            CubeDesc {
                offset: glam::Vec3::new(-2.0, 2.0, 2.0),
                angle: 140.0,
                scale: 1.1,
                rotation: 0.3,
            },
            CubeDesc {
                offset: glam::Vec3::new(2.0, 2.0, 2.0),
                angle: 210.0,
                scale: 0.9,
                rotation: 0.4,
            },
        ];

        let entity_uniform_size = mem::size_of::<EntityUniforms>() as wgpu::BufferAddress;
        let num_entities = 8 + 1 + cube_descs.len() as wgpu::BufferAddress;
        // Make the `uniform_alignment` >= `entity_uniform_size` and aligned to `min_uniform_buffer_offset_alignment`.
        let uniform_alignment = {
            let alignment =
                device.limits().min_uniform_buffer_offset_alignment as wgpu::BufferAddress;
            align_to(entity_uniform_size, alignment)
        };
        // Note: dynamic uniform offsets also have to be aligned to `Limits::min_uniform_buffer_offset_alignment`.
        let entity_uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: num_entities * uniform_alignment,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let index_format = wgpu::IndexFormat::Uint16;

        let mut entities = vec![{
            Entity {
                mx_world: glam::Mat4::IDENTITY,
                rotation_speed: 0.0,
                //color: wgpu::Color::WHITE,
                color: wgpu::Color {
                    r: 0.10,
                    g: 0.00,
                    b: 0.00,
                    a: 1.0,
                },
                vertex_buf: Rc::new(plane_vertex_buf),
                index_buf: Rc::new(plane_index_buf),
                index_format,
                index_count: plane_index_data.len(),
                uniform_offset: 0,
            }
        }];

        for (i, cube) in cube_descs.iter().enumerate() {
            let mx_world = glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::splat(cube.scale),
                glam::Quat::from_axis_angle(
                    cube.offset.normalize(),
                    cube.angle * consts::PI / 180.,
                ),
                cube.offset,
            );
            entities.push(Entity {
                mx_world,
                rotation_speed: cube.rotation,
                color: wgpu::Color::GREEN,
                vertex_buf: Rc::clone(&cube_vertex_buf),
                index_buf: Rc::clone(&cube_index_buf),
                index_format,
                index_count: cube_index_data.len(),
                uniform_offset: ((i + 1) * uniform_alignment as usize) as _,
            });
        }

        let extra_offset = ((cube_descs.len() + 1) * uniform_alignment as usize) as _;

        let entity_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &entity_pipeline.entity_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &entity_uniform_buf,
                    offset: 0,
                    size: wgpu::BufferSize::new(entity_uniform_size),
                }),
            }],
            label: None,
        });

        // Create other resources
        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });

        let shadow_texture = device.create_texture(&wgpu::TextureDescriptor {
            size: Self::SHADOW_SIZE,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: EntityPipeline::SHADOW_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            label: None,
        });
        let shadow_view = shadow_texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut shadow_target_views = (0..2)
            .map(|i| {
                Some(shadow_texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("shadow"),
                    format: None,
                    dimension: Some(wgpu::TextureViewDimension::D2),
                    aspect: wgpu::TextureAspect::All,
                    base_mip_level: 0,
                    mip_level_count: None,
                    base_array_layer: i as u32,
                    array_layer_count: NonZeroU32::new(1),
                }))
            })
            .collect::<Vec<_>>();
        let lights = vec![
            Light {
                pos: glam::Vec3::new(7.0, 10.0, -5.0),
                color: wgpu::Color {
                    r: 0.5,
                    g: 1.0,
                    b: 0.5,
                    a: 1.0,
                },
                fov: 60.0,
                depth: 1.0..20.0,
                target_view: shadow_target_views[0].take().unwrap(),
            },
            Light {
                pos: glam::Vec3::new(-5.0, 10.0, 7.0),
                color: wgpu::Color {
                    r: 1.0,
                    g: 0.5,
                    b: 0.5,
                    a: 1.0,
                },
                fov: 45.0,
                depth: 1.0..20.0,
                target_view: shadow_target_views[1].take().unwrap(),
            },
        ];
        let light_uniform_size =
            (EntityPipeline::MAX_LIGHTS * mem::size_of::<LightRaw>()) as wgpu::BufferAddress;
        let light_storage_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: light_uniform_size,
            usage: if supports_storage_resources {
                wgpu::BufferUsages::STORAGE
            } else {
                wgpu::BufferUsages::UNIFORM
            } | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let shadow_pass = {
            let uniform_size = mem::size_of::<GlobalUniforms>() as wgpu::BufferAddress;

            // Create pipeline layout

            let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: None,
                size: uniform_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Create bind group
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                layout: &entity_pipeline.shadow_bind_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                }],
                label: None,
            });

            // Create the render pipeline

            RenderPass {
                bind_group,
                uniform_buf,
            }
        };

        let eye = glam::Vec3::new(4.0f32, 7.0, 8.0);

        let forward_pass = {
            // Create pipeline layout

            let mx_total = Self::generate_matrix(sc_desc.width as f32 / sc_desc.height as f32, eye);
            let forward_uniforms = GlobalUniforms {
                proj: mx_total.to_cols_array_2d(),
                num_lights: [lights.len() as u32, 0, 0, 0],
            };
            let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Uniform Buffer"),
                contents: bytemuck::bytes_of(&forward_uniforms),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

            // Create bind group
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: &entity_pipeline.forward_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: light_storage_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&shadow_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&shadow_sampler),
                    },
                ],
            });

            // Create the render pipeline

            RenderPass {
                bind_group,
                uniform_buf,
            }
        };

        let framebuffer = Framebuffer::new(device, sc_desc, sample_count);

        {
            grass.update(queue);

            let mut encoder = device.create_command_encoder(&Default::default());
            grass.dispatch(&mut encoder);
            queue.submit(iter::once(encoder.finish()));
        }

        let post = Postprocess::new(
            device,
            sc_desc.format,
            &framebuffer.depth_view,
            &framebuffer.normal_view,
        );

        Self {
            entities,
            lights: LightManager {
                lights,
                dirty: true,
                storage_buf: light_storage_buf,
            },

            framebuffer,

            shadow_pass,
            forward_pass,
            entity_uniform_buf,
            entity_bind_group,

            grass,
            entity_pipeline,

            extra_offset,
            eye,
            show_grass: true,
            show_postprocess: false,

            post,
        }
    }

    fn update(&mut self, event: winit::event::WindowEvent) {
        use winit::event::{ElementState as State, VirtualKeyCode as Key};

        if let winit::event::WindowEvent::KeyboardInput {
            input:
                winit::event::KeyboardInput {
                    state,
                    virtual_keycode: Some(key),
                    ..
                },
            ..
        } = event
        {
            match (state, key) {
                (State::Pressed, Key::W) => self.eye.x += 1.0,
                (State::Pressed, Key::S) => self.eye.x -= 1.0,

                (State::Pressed, Key::A) => self.eye.z += 1.0,
                (State::Pressed, Key::D) => self.eye.z -= 1.0,

                (State::Pressed, Key::Key1) => self.show_grass = !self.show_grass,
                (State::Pressed, Key::Key2) => self.show_postprocess = !self.show_postprocess,

                _ => (),
            }
        }
    }

    fn resize(
        &mut self,
        config: &wgpu::SurfaceConfiguration,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        // update view-projection matrix
        let mx_total = Self::generate_matrix(config.width as f32 / config.height as f32, self.eye);
        let mx_ref: &[f32; 16] = mx_total.as_ref();
        queue.write_buffer(
            &self.forward_pass.uniform_buf,
            0,
            bytemuck::cast_slice(mx_ref),
        );

        self.framebuffer.resize(device, config);

        self.post.resize(
            device,
            queue,
            config.width,
            config.height,
            &self.framebuffer.depth_view,
            &self.framebuffer.normal_view,
        );
    }

    fn render(
        &mut self,

        config: &wgpu::SurfaceConfiguration,
        view: &wgpu::TextureView,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _spawner: &framework::Spawner,
    ) {
        self.grass.update(queue);
        self.post.update(queue, config.width, config.height);

        {
            let mx_total =
                Self::generate_matrix(config.width as f32 / config.height as f32, self.eye);
            let mx_ref: &[f32; 16] = mx_total.as_ref();
            queue.write_buffer(
                &self.forward_pass.uniform_buf,
                0,
                bytemuck::cast_slice(mx_ref),
            );
        }

        // update uniforms
        for entity in self.entities.iter_mut() {
            if entity.rotation_speed != 0.0 {
                let rotation =
                    glam::Mat4::from_rotation_x(entity.rotation_speed * consts::PI / 180.);
                entity.mx_world *= rotation;
            }
            let data = EntityUniforms {
                model: entity.mx_world.to_cols_array_2d(),
                color: [
                    entity.color.r as f32,
                    entity.color.g as f32,
                    entity.color.b as f32,
                    entity.color.a as f32,
                ],
            };
            queue.write_buffer(
                &self.entity_uniform_buf,
                entity.uniform_offset as wgpu::BufferAddress,
                bytemuck::bytes_of(&data),
            );
        }

        queue.write_buffer(
            &self.entity_uniform_buf,
            self.extra_offset as wgpu::BufferAddress,
            bytemuck::bytes_of(&EntityUniforms {
                model: glam::Mat4::IDENTITY.to_cols_array_2d(),
                //color: [0.75, 0.0, 0.0, 1.0],
                color: [0.0, 0.0, 0.75, 1.0],
            }),
        );

        self.lights.update(queue);

        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        encoder.push_debug_group("shadow passes");
        for (i, light) in self.lights.lights.iter().enumerate() {
            encoder.push_debug_group(&format!(
                "shadow pass {} (light at position {:?})",
                i, light.pos
            ));

            // The light uniform buffer already has the projection,
            // let's just copy it over to the shadow uniform buffer.
            encoder.copy_buffer_to_buffer(
                &self.lights.storage_buf,
                (i * mem::size_of::<LightRaw>()) as wgpu::BufferAddress,
                &self.shadow_pass.uniform_buf,
                0,
                64,
            );

            encoder.insert_debug_marker("render entities");
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &light.target_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: true,
                        }),
                        stencil_ops: None,
                    }),
                });
                pass.set_pipeline(&self.entity_pipeline.bake);
                pass.set_bind_group(0, &self.shadow_pass.bind_group, &[]);

                for entity in &self.entities {
                    pass.set_bind_group(1, &self.entity_bind_group, &[entity.uniform_offset]);
                    pass.set_index_buffer(entity.index_buf.slice(..), entity.index_format);
                    pass.set_vertex_buffer(0, entity.vertex_buf.slice(..));
                    pass.draw_indexed(0..entity.index_count as u32, 0, 0..1);
                }

                if false {
                    pass.set_pipeline(&self.grass.pipeline.bake);
                    pass.set_bind_group(1, &self.entity_bind_group, &[self.extra_offset]);
                    pass.set_vertex_buffer(0, self.grass.dst_vertices_buf.slice(..));
                    pass.set_index_buffer(
                        self.grass.indices_buf.slice(..),
                        wgpu::IndexFormat::Uint32,
                    );
                    pass.draw_indexed_indirect(&self.grass.dst_indirect_buf, 0);
                }
            }

            encoder.pop_debug_group();
        }
        encoder.pop_debug_group();

        encoder.push_debug_group("grass dispatch");
        {
            self.grass.dispatch(&mut encoder);
        }
        encoder.pop_debug_group();

        // forward pass
        encoder.push_debug_group("forward rendering pass");
        {
            let (view, resolve_target) = self.framebuffer.target(view);

            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.2,
                                b: 0.3,
                                a: 1.0,
                            }),
                            store: true,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.framebuffer.normal_view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: true,
                        },
                    }),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.framebuffer.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            pass.set_pipeline(&self.entity_pipeline.draw);
            pass.set_bind_group(0, &self.forward_pass.bind_group, &[]);

            if true {
                for (_index, entity) in self.entities.iter().enumerate() {
                    pass.set_bind_group(1, &self.entity_bind_group, &[entity.uniform_offset]);
                    pass.set_index_buffer(entity.index_buf.slice(..), entity.index_format);
                    pass.set_vertex_buffer(0, entity.vertex_buf.slice(..));

                    pass.draw_indexed(0..entity.index_count as u32, 0, 0..1);
                }
            }

            if self.show_grass {
                pass.set_pipeline(&self.grass.pipeline.draw);
                pass.set_bind_group(1, &self.entity_bind_group, &[self.extra_offset]);
                pass.set_vertex_buffer(0, self.grass.dst_vertices_buf.slice(..));
                pass.set_index_buffer(self.grass.indices_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed_indirect(&self.grass.dst_indirect_buf, 0);
            }
        }
        encoder.pop_debug_group();

        //queue.submit(iter::once(encoder.finish()));
        //let mut encoder = device.create_command_encoder(&Default::default());

        encoder.push_debug_group("postprocess");
        if self.show_postprocess {
            self.post.draw(&mut encoder, view);
        }
        encoder.pop_debug_group();

        queue.submit(iter::once(encoder.finish()));
    }
}

struct LightManager {
    lights: Vec<Light>,
    dirty: bool,
    storage_buf: wgpu::Buffer,
}

impl LightManager {
    pub fn update(&mut self, queue: &wgpu::Queue) {
        if self.dirty {
            self.dirty = false;
            for (i, light) in self.lights.iter().enumerate() {
                queue.write_buffer(
                    &self.storage_buf,
                    (i * mem::size_of::<LightRaw>()) as wgpu::BufferAddress,
                    bytemuck::bytes_of(&light.to_raw()),
                );
            }
        }
    }
}

struct Framebuffer {
    sample_count: u32,
    color_target: wgpu::TextureView,

    normal_view: wgpu::TextureView,

    depth_view: wgpu::TextureView,
}

impl Framebuffer {
    const NORMAL: wgpu::TextureFormat = wgpu::TextureFormat::Rgb10a2Unorm;

    fn new(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration, sample_count: u32) -> Self {
        let color_target = Self::multisampled(
            device,
            config,
            sample_count,
            config.format,
            wgpu::TextureUsages::empty(),
        );
        let normal_view = Self::multisampled(
            device,
            config,
            sample_count,
            Self::NORMAL,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let depth_view = Self::depth(device, config, sample_count);

        Self {
            sample_count,
            color_target,
            normal_view,
            depth_view,
        }
    }

    fn resize(&mut self, device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) {
        self.color_target = Self::multisampled(
            device,
            config,
            self.sample_count,
            config.format,
            wgpu::TextureUsages::empty(),
        );
        self.normal_view = Self::multisampled(
            device,
            config,
            self.sample_count,
            Self::NORMAL,
            wgpu::TextureUsages::TEXTURE_BINDING,
        );
        self.depth_view = Self::depth(device, config, self.sample_count);
    }

    fn target<'a>(
        &'a self,
        view: &'a wgpu::TextureView,
    ) -> (&'a wgpu::TextureView, Option<&'a wgpu::TextureView>) {
        if self.sample_count <= 1 {
            (view, None)
        } else {
            (&self.color_target, Some(view))
        }
    }

    fn multisampled(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
        format: wgpu::TextureFormat,
        usages: wgpu::TextureUsages,
    ) -> wgpu::TextureView {
        let size = wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };
        let multisampled_frame_descriptor = &wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | usages,
        };

        device
            .create_texture(multisampled_frame_descriptor)
            .create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn depth(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        sample_count: u32,
    ) -> wgpu::TextureView {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count,
            dimension: wgpu::TextureDimension::D2,
            format: EntityPipeline::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        });

        texture.create_view(&wgpu::TextureViewDescriptor::default())
    }
}
