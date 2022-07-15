// https://roystan.net/articles/outline-shader.html

struct Params {
    view_space_directon: vec4<f32>,
    color: vec4<f32>,

    scale: i32,

    pad1: f32,
    pad2: f32,
    pad3: f32,

    depth_threshold: f32, // 0..1
    depth_normal_threshold: f32,
    depth_normal_threshold_scale: f32,
    normal_threshold: f32, // 0..1
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var depth_single: texture_depth_2d;
@group(0) @binding(1) var depth_multi: texture_depth_multisampled_2d;
@group(0) @binding(2) var normal_single: texture_2d<f32>;
@group(0) @binding(2) var normal_multi: texture_multisampled_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let u = (vertex_index << 1u) & 2u;
    let v = vertex_index & 2u;
    let u = f32( 2 * i32(u) - 1);
    let v = f32(-2 * i32(v) + 1);

    return vec4<f32>(u, v, 0.0, 1.0);
}

fn edge_detect(depth: array<f32, 4>, normal: array<vec3<f32>, 4>) -> f32 {
    let view_space_directon = normalize(params.view_space_directon.xyz);

    // Transform the view normal from the 0...1 range to the -1...1 range.
    let view_normal = normal[0] * 2.0 - 1.0;
    let NdotV = 1.0 - dot(view_normal, -view_space_directon);

    // Return a value in the 0...1 range depending on where NdotV lies between depth_normal_threshold and 1.
    // Then scale the threshold, and add 1 so that it is in the range of 1...normal_threshold_scale + 1.
    let normal_threshold = clamp((NdotV - params.depth_normal_threshold) / (1.0 - params.depth_normal_threshold), 0.0, 1.0);
    let normal_threshold = normal_threshold * params.depth_normal_threshold_scale + 1.0;

    // Modulate the threshold by the existing depth value;
    // pixels further from the screen will require smaller differences to draw an edge.
    let depth_threshold = params.depth_threshold * depth[0] * normal_threshold;

    // edge_depth is calculated using the Roberts cross operator.
    // The same operation is applied to the normal below.
    // https://en.wikipedia.org/wiki/Roberts_cross
    let depth_a = depth[1] - depth[2];
    let depth_b = depth[0] - depth[3];
    let edge_depth = sqrt(depth_a * depth_a + depth_b * depth_b) * 100.0;
    let edge_depth = step(depth_threshold, edge_depth);

    // Dot the finite differences with themselves to transform the three-dimensional values to scalars.
    let normal_a = normal[1] - normal[2];
    let normal_b = normal[0] - normal[3];
    let edge_normal = sqrt(dot(normal_a, normal_a) + dot(normal_b, normal_b));
    let edge_normal = step(params.normal_threshold, edge_normal);

    return max(edge_depth, edge_normal);
}

@fragment
fn fs_main_single(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(position.xy);

    let scale = max(1, params.scale);
    let tl = vec2<i32>( scale, -scale);
    let rt = vec2<i32>( scale,  scale);
    let lb = vec2<i32>(-scale, -scale);
    let br = vec2<i32>(-scale,  scale);

    let depth = array<f32, 4>(
        1.0 - textureLoad(depth_single, px + tl, 0),
        1.0 - textureLoad(depth_single, px + rt, 0),
        1.0 - textureLoad(depth_single, px + lb, 0),
        1.0 - textureLoad(depth_single, px + br, 0),
    );

    let normal = array<vec3<f32>, 4>(
        textureLoad(normal_single, px + tl, 0).xyz,
        textureLoad(normal_single, px + rt, 0).xyz,
        textureLoad(normal_single, px + lb, 0).xyz,
        textureLoad(normal_single, px + br, 0).xyz,
    );

    let edge = edge_detect(depth, normal);
    return vec4<f32>(params.color.rgb, params.color.a * edge);
}

@fragment
fn fs_main_multi(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(position.xy);

    let scale = max(0, params.scale);
    let tl = vec2<i32>( scale, -scale);
    let rt = vec2<i32>( scale,  scale);
    let lb = vec2<i32>(-scale, -scale);
    let br = vec2<i32>(-scale,  scale);

    let depth = array<f32, 4>(
        1.0 - textureLoad(depth_multi, px + tl, 0),
        1.0 - textureLoad(depth_multi, px + rt, 1),
        1.0 - textureLoad(depth_multi, px + lb, 2),
        1.0 - textureLoad(depth_multi, px + br, 3),
    );

    let normal = array<vec3<f32>, 4>(
        textureLoad(normal_multi, px + tl, 0).xyz,
        textureLoad(normal_multi, px + rt, 1).xyz,
        textureLoad(normal_multi, px + lb, 2).xyz,
        textureLoad(normal_multi, px + br, 3).xyz,
    );

    let edge = edge_detect(depth, normal);
    return vec4<f32>(params.color.rgb * edge, params.color.a * edge);
}