// https://roystan.net/articles/outline-shader.html

struct ScreenUniforms {
    size: vec2<f32>,
    inv_size: vec2<f32>,
    time: f32,
    pad: f32,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0)       uv: vec2<f32>,
    @location(1)       coord: vec2<f32>,
}

struct FragmentInput {
    @builtin(position) position: vec4<f32>,
    @location(0)       uv: vec2<f32>,
    @location(1)       px: vec2<f32>,
}

@group(0) @binding(0) var<uniform> u_screen: ScreenUniforms;
//@group(0) @binding(1) var depth_texture: texture_multisampled_2d<f32>;
//@group(0) @binding(1) var depth_texture: texture_2d<f32>;
@group(0) @binding(1) var framebuffer_sampler: sampler;
@group(0) @binding(2) var depth_texture: texture_depth_2d;
@group(0) @binding(3) var normal_texture: texture_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let u = (vertex_index << 1u) & 2u;
    let v = vertex_index & 2u;
    let u = f32( 2 * i32(u) - 1);
    let v = f32(-2 * i32(v) + 1);

    let position = vec4<f32>(u, v, 0.0, 1.0);
    let uv = vec2<f32>(u, v) * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5, 0.5);
    let px = u_screen.size * uv;

    return VertexOutput(position, uv, px);
}

fn remap(input: f32, in_range: vec2<f32>, out_range: vec2<f32>) -> f32 {
    return out_range.x + (input - in_range.x) * (out_range.y - out_range.x) / (in_range.y - in_range.x);
}


@fragment
fn fs_main(in: FragmentInput) -> @location(0) vec4<f32> {
    //let coord = vec2<i32>(i32(in.px.x), i32(in.px.y));


    //let params_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    let params_color = vec4<f32>(0.0, 0.0, 0.0, 0.5);

    // Number of pixels between samples that are tested for an edge. When this value is 1, tested samples are adjacent.
    let params_scale = 1.0;
    // Difference between depth values, scaled by the current depth, required to draw an edge.
    let params_depth_threshold = 1.5;
    // The value at which the dot product between the surface normal and the view direction will affect
    // the depth threshold. This ensures that surfaces at right angles to the camera require a larger depth threshold to draw
    // an edge, avoiding edges being drawn along slopes.
    let params_depth_normal_threshold = 0.5; // 0..1
    // Scale the strength of how much the depthNormalThreshold affects the depth threshold.
    let params_depth_normal_threshold_scale = 7.0;
    // Larger values will require the difference between normals to be greater to draw an edge.
    let params_normal_threshold = 0.4; // 0..1


    let half_scale_floor = floor(params_scale * 0.5);
    let half_scale_ceil = ceil(params_scale * 0.5);

    let texel_size = u_screen.inv_size;

    let bl = in.uv - vec2<f32>( texel_size.x, texel_size.y) * half_scale_floor;
    let tr = in.uv + vec2<f32>( texel_size.x, texel_size.y) * half_scale_ceil;
    let br = in.uv + vec2<f32>( texel_size.x * half_scale_ceil, -texel_size.y * half_scale_floor);
    let tl = in.uv + vec2<f32>(-texel_size.x * half_scale_floor, texel_size.y * half_scale_ceil);


    let normal0 = textureSample(normal_texture, framebuffer_sampler, bl).xyz;
    let normal1 = textureSample(normal_texture, framebuffer_sampler, tr).xyz;
    let normal2 = textureSample(normal_texture, framebuffer_sampler, br).xyz;
    let normal3 = textureSample(normal_texture, framebuffer_sampler, tl).xyz;

    let depth0 = 1.0 - textureSample(depth_texture, framebuffer_sampler, bl);
    let depth1 = 1.0 - textureSample(depth_texture, framebuffer_sampler, tr);
    let depth2 = 1.0 - textureSample(depth_texture, framebuffer_sampler, br);
    let depth3 = 1.0 - textureSample(depth_texture, framebuffer_sampler, tl);

    let view_space_directon = vec3<f32>(4.0, 7.0, 8.0);
    let view_space_directon = normalize(view_space_directon);

    // Transform the view normal from the 0...1 range to the -1...1 range.
    let view_normal = normal0 * 2.0 - 1.0;
    let NdotV = 1.0 - dot(view_normal, -view_space_directon);


    // Return a value in the 0...1 range depending on where NdotV lies
    // between _DepthNormalThreshold and 1.
    let normal_threshold_01 = clamp((NdotV - params_depth_normal_threshold) / (1.0 - params_depth_normal_threshold), 0.0, 1.0);
    // Scale the threshold, and add 1 so that it is in the range of 1..._NormalThresholdScale + 1.
    let normal_threshold = normal_threshold_01 * params_depth_normal_threshold_scale + 1.0;


    // Modulate the threshold by the existing depth value;
    // pixels further from the screen will require smaller differences to draw an edge.
    let depth_threshold = params_depth_threshold * depth0 * normal_threshold;

    let depth_finite_difference_10 = depth1 - depth0;
    let depth_finite_difference_32 = depth3 - depth2;


    // edgeDepth is calculated using the Roberts cross operator.
    // The same operation is applied to the normal below.
    // https://en.wikipedia.org/wiki/Roberts_cross
    let edge_depth = sqrt(pow(depth_finite_difference_10, 2.0) + pow(depth_finite_difference_32, 2.0)) * 100.0;
    let edge_depth = step(depth_threshold, edge_depth);


    let normal_finite_difference_10 = normal1 - normal0;
    let normal_finite_difference_32 = normal3 - normal2;
    // Dot the finite differences with themselves to transform the
    // three-dimensional values to scalars.
    let edge_normal = sqrt(
        dot(normal_finite_difference_10, normal_finite_difference_10) +
        dot(normal_finite_difference_32, normal_finite_difference_32));

    let edge_normal = step(params_normal_threshold, edge_normal);

    let edge = max(edge_depth, edge_normal);

    let edge_color = vec4<f32>(params_color.rgb, params_color.a * edge);

    return edge_color;
}