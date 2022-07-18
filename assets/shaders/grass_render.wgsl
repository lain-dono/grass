#import bevy_pbr::mesh_types
#import bevy_pbr::mesh_view_bindings

@group(1) @binding(0)
var<uniform> mesh: Mesh;

// NOTE: Bindings must come before functions that use them!
#import bevy_pbr::mesh_functions

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,

    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
};

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = mesh_position_local_to_clip(mesh.model, vec4<f32>(vertex.position, 1.0));
    out.world_position = mesh_position_local_to_world(mesh.model, vec4<f32>(vertex.position, 1.0));
    out.world_normal = mesh_normal_local_to_world(vertex.normal);
    out.uv = vertex.uv;
    out.color = vec4<f32>(vec3<f32>(0.079, 0.245, 0.160)*vertex.uv.y, 1.0);
    return out;
}

#import bevy_pbr::pbr_types
#import bevy_pbr::utils
#import bevy_pbr::clustered_forward
#import bevy_pbr::lighting
#import bevy_pbr::shadows
#import bevy_pbr::pbr_functions

struct Time {
    time_since_startup: f32,
}

struct FragInput {
    @builtin(position) frag_coord: vec4<f32>,
    @builtin(front_facing) is_front: bool,

    @location(0) world_position: vec4<f32>,
    @location(1) world_normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
}

@fragment
fn fragment(in: FragInput) -> @location(0) vec4<f32> {
    //return vec4<f32>(0.5, 0.5, 0.5, 1.0);

    // Prepare a 'processed' StandardMaterial by sampling all textures to resolve
    // the material members
    var pbr_input: PbrInput = pbr_input_new();

    //pbr_input.material.base_color = vec4<f32>(1.0, 1.0, 1.0, 1.0);
    pbr_input.material.base_color = in.color;

#ifdef VERTEX_COLORS
    pbr_input.material.base_color = pbr_input.material.base_color * in.color;
#endif

    //pbr_input.material.reflectance = 0.1;
    pbr_input.material.perceptual_roughness = 1.0;


    pbr_input.frag_coord = in.frag_coord;
    pbr_input.world_position = in.world_position;
    pbr_input.world_normal = in.world_normal;

    pbr_input.is_orthographic = view.projection[3].w == 1.0;

    pbr_input.N = prepare_normal(
        pbr_input.material.flags,
        in.world_normal,
        in.uv,
        in.is_front,
    );

    pbr_input.V = calculate_view(in.world_position, pbr_input.is_orthographic);

    return tone_mapping(pbr(pbr_input));
}


//  struct VertexNormalOutput {
//      @builtin(position) clip_position: vec4<f32>,
//      @location(0) world_normal: vec3<f32>,
//  };

//  @vertex
//  fn vertex_normal_pass(vertex: Vertex) -> VertexNormalOutput {
//      return VertexNormalOutput(
//          mesh_position_local_to_clip(mesh.model, vec4<f32>(vertex.position, 1.0)),
//          mesh_normal_local_to_world(vertex.normal)
//      );
//  }

//  @fragment
//  fn fragment_normal_pass(@location(0) world_normal: vec3<f32>) -> @location(0) vec4<f32> {
//      return vec4<f32>(world_normal, 1.0);
//  }