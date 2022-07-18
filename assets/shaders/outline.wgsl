
@vertex
fn vertex(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let u = (vertex_index << 1u) & 2u;
    let v = vertex_index & 2u;
    let u = f32( 2 * i32(u) - 1);
    let v = f32(-2 * i32(v) + 1);
    return vec4<f32>(u, v, 0.0, 1.0);
}


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

#ifdef MULTISAMPLED
@group(0) @binding(1) var depth: texture_depth_multisampled_2d;
@group(0) @binding(2) var normal: texture_multisampled_2d<f32>;
#else
@group(0) @binding(1) var depth: texture_depth_2d;
@group(0) @binding(2) var normal: texture_2d<f32>;
#endif


@fragment
fn fragment(@builtin(position) position: vec4<f32>, @builtin(sample_index) sample_index: u32) -> @location(0) vec4<f32> {
    let px = vec2<i32>(position.xy);
    let sample_index = i32(sample_index);

    let scale = max(1, params.scale);

    let tl = vec2<i32>( scale, -scale);
    let rt = vec2<i32>( scale,  scale);
    let lb = vec2<i32>(-scale, -scale);
    let br = vec2<i32>(-scale,  scale);

    let depth_0 = 1.0 - textureLoad(depth, px + tl, sample_index);
    let depth_1 = 1.0 - textureLoad(depth, px + rt, sample_index);
    let depth_2 = 1.0 - textureLoad(depth, px + lb, sample_index);
    let depth_3 = 1.0 - textureLoad(depth, px + br, sample_index);

    let normal_0 = textureLoad(normal, px + tl, sample_index).xyz;
    let normal_1 = textureLoad(normal, px + rt, sample_index).xyz;
    let normal_2 = textureLoad(normal, px + lb, sample_index).xyz;
    let normal_3 = textureLoad(normal, px + br, sample_index).xyz;

    let view_space_directon = normalize(params.view_space_directon.xyz);

    // Transform the view normal from the 0...1 range to the -1...1 range.
    let view_normal = normal_0 * 2.0 - 1.0;
    let NdotV = 1.0 - dot(view_normal, -view_space_directon);

    // Return a value in the 0...1 range depending on where NdotV lies between depth_normal_threshold and 1.
    // Then scale the threshold, and add 1 so that it is in the range of 1...normal_threshold_scale + 1.
    let normal_threshold = clamp((NdotV - params.depth_normal_threshold) / (1.0 - params.depth_normal_threshold), 0.0, 1.0);
    let normal_threshold = normal_threshold * params.depth_normal_threshold_scale + 1.0;

    // Modulate the threshold by the existing depth value;
    // pixels further from the screen will require smaller differences to draw an edge.
    let depth_threshold = params.depth_threshold * depth_0 * normal_threshold;

    // edge_depth is calculated using the Roberts cross operator.
    // The same operation is applied to the normal below.
    // https://en.wikipedia.org/wiki/Roberts_cross
    let depth_a = depth_1 - depth_2;
    let depth_b = depth_0 - depth_3;
    let edge_depth = sqrt(depth_a * depth_a + depth_b * depth_b) * 100.0;
    let edge_depth = step(depth_threshold, edge_depth);

    // Dot the finite differences with themselves to transform the three-dimensional values to scalars.
    let normal_a = normal_1 - normal_2;
    let normal_b = normal_0 - normal_3;
    let edge_normal = sqrt(dot(normal_a, normal_a) + dot(normal_b, normal_b));
    let edge_normal = step(params.normal_threshold, edge_normal);

    let edge = max(edge_depth, edge_normal);

    return vec4<f32>(params.color.rgb * edge, params.color.a * edge);
}