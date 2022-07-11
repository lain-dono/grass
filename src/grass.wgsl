struct Globals {
    view_proj: mat4x4<f32>,
    num_lights: vec4<u32>,
};

struct Entity {
    world: mat4x4<f32>,
    color: vec4<f32>,
};

struct Light {
    proj: mat4x4<f32>,
    pos: vec4<f32>,
    color: vec4<f32>,
};

let c_ambient: vec3<f32> = vec3<f32>(0.05, 0.05, 0.05);
let c_max_lights: u32 = 10u;

@group(0) @binding(0) var<uniform> u_globals: Globals;
@group(0) @binding(1) var<storage, read> s_lights: array<Light>;
@group(0) @binding(1) var<uniform> u_lights: array<Light, 10>; // Used when storage types are not supported
@group(0) @binding(2) var texture_shadow: texture_depth_2d_array;
@group(0) @binding(3) var sampler_shadow: sampler_comparison;
@group(1) @binding(0) var<uniform> u_entity: Entity;


@vertex fn vs_bake(@location(0) position: vec3<f32>) -> @builtin(position) vec4<f32> {
    return u_globals.view_proj * u_entity.world * vec4<f32>(position, 1.0);
}

struct VertexOutput {
    @builtin(position) proj_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec4<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_draw(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
) -> VertexOutput {
    let w = u_entity.world;
    let world_pos = u_entity.world * vec4<f32>(position, 1.0);
    var result: VertexOutput;
    result.world_normal = mat3x3<f32>(w.x.xyz, w.y.xyz, w.z.xyz) * vec3<f32>(normal.xyz);
    result.world_position = world_pos;
    result.proj_position = u_globals.view_proj * world_pos;
    result.color = mix(u_entity.color, vec4<f32>(1.0, 0.0, 0.0, 1.0), uv.y);
    return result;
}

// fragment shader


fn fetch_shadow(light_id: u32, homogeneous_coords: vec4<f32>) -> f32 {
    if (homogeneous_coords.w <= 0.0) {
        return 1.0;
    }
    // compensate for the Y-flip difference between the NDC and texture coordinates
    let flip_correction = vec2<f32>(0.5, -0.5);
    // compute texture coordinates for shadow lookup
    let proj_correction = 1.0 / homogeneous_coords.w;
    let light_local = homogeneous_coords.xy * flip_correction * proj_correction + vec2<f32>(0.5, 0.5);
    // do the lookup, using HW PCF and comparison
    return textureSampleCompareLevel(texture_shadow, sampler_shadow, light_local, i32(light_id), homogeneous_coords.z * proj_correction);
}

@fragment
fn fs_draw(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(vertex.world_normal);
    // accumulate color
    var color: vec3<f32> = c_ambient;
    for(var i = 0u; i < min(u_globals.num_lights.x, c_max_lights); i += 1u) {
        let light = s_lights[i];
        // project into the light space
        let shadow = fetch_shadow(i, light.proj * vertex.world_position);
        // compute Lambertian diffuse term
        let light_dir = normalize(light.pos.xyz - vertex.world_position.xyz);
        let diffuse = max(0.0, dot(normal, light_dir));
        // add light contribution
        color += shadow * diffuse * light.color.xyz;
    }
    // multiply the light by material color
    return vec4<f32>(color, 1.0) * vertex.color;
}

@fragment
fn fs_main_storage(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(vertex.world_normal);
    // accumulate color
    var color: vec3<f32> = c_ambient;
    for(var i = 0u; i < min(u_globals.num_lights.x, c_max_lights); i += 1u) {
        let light = s_lights[i];
        // project into the light space
        let shadow = fetch_shadow(i, light.proj * vertex.world_position);
        // compute Lambertian diffuse term
        let light_dir = normalize(light.pos.xyz - vertex.world_position.xyz);
        let diffuse = max(0.0, dot(normal, light_dir));
        // add light contribution
        color += shadow * diffuse * light.color.xyz;
    }
    // multiply the light by material color
    return vec4<f32>(color, 1.0) * vertex.color;
}

// The fragment entrypoint used when storage buffers are not available for the lights
@fragment
fn fs_main_uniform(vertex: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(vertex.world_normal);
    var color: vec3<f32> = c_ambient;
    for(var i = 0u; i < min(u_globals.num_lights.x, c_max_lights); i += 1u) {
        // This line is the only difference from the entrypoint above. It uses the lights
        // uniform instead of the lights storage buffer
        let light = u_lights[i];
        let shadow = fetch_shadow(i, light.proj * vertex.world_position);
        let light_dir = normalize(light.pos.xyz - vertex.world_position.xyz);
        let diffuse = max(0.0, dot(normal, light_dir));
        color += shadow * diffuse * light.color.xyz;
    }
    return vec4<f32>(color, 1.0) * vertex.color;
}

// compute shader

struct Params {
    time: f32,
    length: u32,

    blades: u32,
    blade_radius: f32,
    blade_forward: f32,
    blade_curve: f32,

    wind_speed: f32,
    wind_strength: f32,
}

struct DrawIndexedIndirect {
    vertex_count: atomic<u32>,
    instance_count: u32,
    base_index: u32,
    vertex_offset: i32,
    base_instance: u32,
}

@group(0) @binding(0) var<uniform>             params: Params;
@group(0) @binding(1) var<storage, read>       src_vertices: array<array<f32, 6>>; // position + normal
@group(0) @binding(2) var<storage, read_write> dst_vertices: array<array<f32, 8>>; // position + normal + uvs
@group(0) @binding(3) var<storage, read_write> dst_vertices_count: atomic<u32>;
@group(0) @binding(4) var<storage, read_write> dst_indirect: DrawIndexedIndirect;


@compute @workgroup_size(1, 1, 1)
fn cs_main_init() {
    atomicStore(&dst_vertices_count, 0u);
    atomicStore(&dst_indirect.vertex_count, 0u);
    dst_indirect.instance_count = 1u;
    dst_indirect.base_index = 0u;
    dst_indirect.vertex_offset = 0;
    dst_indirect.base_instance = 0u;
}

fn set_vertex(index: u32, normal: vec3<f32>, position: vec3<f32>, texcoord: vec2<f32>) {
    dst_vertices[index] = array<f32, 8>(
        position.x, position.y, position.z,
        normal.x, normal.y, normal.z,
        texcoord.x, texcoord.y,
    );
}

fn face_normal(a: vec3<f32>, b: vec3<f32>, c: vec3<f32>) -> vec3<f32> {
    return normalize(cross(b - a, c - a));
}

// A function to compute an rotation matrix which rotates a point
// by angle radians around the given axis
// By Keijiro Takahashi
fn angle_axis_3x3(angle: f32, axis: vec3<f32>) -> mat3x3<f32> {
    // float c, s; sincos(angle, s, c);
    let s = sin(angle);
    let c = cos(angle);

    let s = axis * s;
    let t = axis * (1.0 - c);

    return mat3x3<f32>(
        t.x * axis.x + c  , t.y * axis.x - s.z, t.z * axis.x + s.y,
        t.x * axis.y + s.z, t.y * axis.y + c  , t.z * axis.y - s.x,
        t.x * axis.z - s.y, t.y * axis.z + s.x, t.z * axis.z + c ,
    );
}

let PI: f32  = 3.14159265358979323846;
let TAU: f32 = 6.28318530717958647693;


@compute @workgroup_size(256)
fn cs_main_fill(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let src_index = global_id.x;

    if (src_index >= params.length) {
        return;
    }

    let src_vertex = src_vertices[src_index];
    let src_position = vec3<f32>(src_vertex[0], src_vertex[1], src_vertex[2]);
    let src_normal   = vec3<f32>(src_vertex[3], src_vertex[4], src_vertex[5]);

    let segments_per_blade = 5u;
    let top_vtx_offset = segments_per_blade * 2u;
    let vtx_per_blade  = segments_per_blade * 2u + 1u;
    let idx_per_blade  = segments_per_blade * 2u + 2u;

    let dst_index = atomicAdd(&dst_vertices_count, vtx_per_blade);

    let rand_seed = fract(sin(dot(src_position.xyz, vec3<f32>(12.9898, 78.233, 53.539))) * 43758.5453);

    let blade_bottom_width = 0.50;
    let blade_width  = 0.15;
    let blade_height = 2.00;

    // Wind

    let wind_speed = params.time * params.wind_speed;
    let wind_sin =
        sin(wind_speed + src_position.x      ) +
        sin(wind_speed + src_position.z * 2.0) + // z
        sin(wind_speed * 0.1 + src_position.x);
    let wind_cos =
        cos(wind_speed + src_position.x * 2.0) +
        cos(wind_speed + src_position.z      ); // z

    //let wind = vec3<f32>(wind_x, 0.0, wind_z) * params_wind_strength;
    let wind = vec3<f32>(wind_sin, wind_cos, 0.0) * params.wind_strength;

    let rotation_axis = vec3<f32>(0.0, 1.0, -0.1);
    //let rotation_axis = vec3<f32>(-0.1, 0.0, 1.0);

    var displacement = vec3<f32>(0.0) + wind;

    var position: array<vec3<f32>, 55u>;
    var texcoord: array<vec2<f32>, 55u>;

    var blade_index = 0u;
    loop {
        if (blade_index >= params.blades) { break; }

        // set rotation and radius of the blades

        let blade_rotation = angle_axis_3x3(rand_seed * TAU + f32(blade_index), rotation_axis);
        let blade_radius = f32(blade_index) / f32(params.blades);
        let blade_offset = (1.0 - blade_radius) * params.blade_radius;

        var segment_index = 0u;
        loop {
            if (segment_index >= segments_per_blade) { break; }

            let taper_width = f32(segment_index) / f32(segments_per_blade);

            // the first segment is thinner
            let first_thinner = select(1.0, blade_bottom_width, segment_index == 0u);
            let width = blade_width * (1.0 - taper_width) * first_thinner;
            let height = blade_height * taper_width;

            let forward = blade_offset + pow(abs(taper_width), params.blade_curve) * params.blade_forward;

            // first grass (0) segment does not get displaced by interactor
            let translation = src_position + select(displacement * taper_width, vec3<f32>(0.0), segment_index == 0u);

            let offset = segment_index * 2u;

            position[offset + 0u] = translation + vec3<f32>( width, height, forward) * blade_rotation;
            position[offset + 1u] = translation + vec3<f32>(-width, height, forward) * blade_rotation;

            texcoord[offset + 0u] = vec2<f32>(0.0, taper_width);
            texcoord[offset + 1u] = vec2<f32>(1.0, taper_width);

            continuing { segment_index += 1u; }
        }

        // top vertex
        let translation = src_position + displacement;
        let forward = blade_offset + params.blade_forward;
        let local_displacement = vec3<f32>(0.0, blade_height, forward);
        position[top_vtx_offset] = translation + local_displacement * blade_rotation;
        texcoord[top_vtx_offset] = vec2<f32>(0.5, 1.0);

        continuing { blade_index += 1u; }
    }

    var i = 0u;
    loop {
        if (i >= vtx_per_blade) { break; }
        set_vertex(dst_index + i, src_normal, position[i], texcoord[i]);
        continuing { i += 1u; }
    }

    atomicAdd(&dst_indirect.vertex_count, idx_per_blade);
}