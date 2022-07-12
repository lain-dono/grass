// Blend SrcAlpha OneMinusSrcAlpha

struct Params {
    transform: mat4x4<f32>,
    tint: vec4<f32>,
    inv_size: vec2<f32>,
    strength: f32,
    znear: f32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var depth_single: texture_depth_2d;
@group(0) @binding(1) var depth_multi: texture_depth_multisampled_2d;


@vertex
fn vs_main(@location(0) position: vec4<i32>) -> @builtin(position) vec4<f32> {
    return params.transform * vec4<f32>(position);
}

@fragment
fn fs_main_single(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let depth = textureLoad(depth_single, vec2<i32>(position.xy), 0);
    let depth = params.znear / (1.0 - depth);
    let z = params.znear / (1.0 - position.z);

    // fog by comparing depth and screenposition
    let fog = params.strength * (depth - z);

    // add the color & clamp to prevent weird artifacts
    return clamp(params.tint * fog, vec4<f32>(0.0), vec4<f32>(1.0));
}

@fragment
fn fs_main_multi(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let depth = textureLoad(depth_multi, vec2<i32>(position.xy), 0);
    let depth = params.znear / (1.0 - depth);
    let z = params.znear / (1.0 - position.z);

    // fog by comparing depth and screenposition
    let fog = params.strength * (depth - z);

    // add the color & clamp to prevent weird artifacts
    return clamp(params.tint * fog, vec4<f32>(0.0), vec4<f32>(1.0));
}
