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
@group(0) @binding(2) var texture_shadow: texture_depth_2d_array;
@group(0) @binding(3) var sampler_shadow: sampler_comparison;
@group(1) @binding(0) var<uniform> u_entity: Entity;

@vertex fn vs_bake(@location(0) position: vec4<i32>) -> @builtin(position) vec4<f32> {
    return u_globals.view_proj * u_entity.world * vec4<f32>(position);
}

struct VertexOutput {
    @builtin(position) proj_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) world_position: vec4<f32>,
    @location(2) color: vec4<f32>,
};

@vertex
fn vs_draw(
    @location(0) position: vec4<i32>,
    @location(1) normal: vec4<i32>,
) -> VertexOutput {
    let w = u_entity.world;
    let world_pos = u_entity.world * vec4<f32>(position);
    var result: VertexOutput;
    result.world_normal = mat3x3<f32>(w.x.xyz, w.y.xyz, w.z.xyz) * vec3<f32>(normal.xyz);
    result.world_position = world_pos;
    result.proj_position = u_globals.view_proj * world_pos;
    result.color = u_entity.color;
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


struct FragmentOutput {
    @location(0) color: vec4<f32>,
    @location(1) normal: vec4<f32>,
}

@fragment
fn fs_draw(vertex: VertexOutput) -> FragmentOutput {
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
    let color = vec4<f32>(color, 1.0) * vertex.color;

    return FragmentOutput(color, vec4<f32>(normal, 0.0));
}

//  // Tags{ "Queue" = "Transparent"}
//  // LOD 200
//  // Blend SrcAlpha OneMinusSrcAlpha
//  // CGPROGRAM

//  struct Params {
//      // Water
//      tcolor: vec4<f32>, // ("Deep Tint", Color) = (0,1,1,1)
//      water_color: vec4<f32>, // ("Edge Tint", Color) = (0,0.6,1,1)
//      depth_offset: f32, // ("Depth Offset", Range(-10,10)) = 0
//      stretch: f32, // ("Depth Stretch", Range(0,5)) = 2
//      brightness: f32, // ("Water Brightness", Range(0.5,2)) = 1.2

//      // Surface Noise and Movement
//      hor_speed: f32, // ("Horizontal Flow Speed", Range(0,4)) = 0.14
//      vert_speed: f32, // ("Vertical Flow Speed", Range(0,60)) = 6.8
//      top_scale: f32, // ("Top Noise Scale", Range(0,1)) = 0.4
//      noise_scale: f32, // ("Side Noise Scale", Range(0,1)) = 0.04
//      //[Toggle(VERTEX)] _VERTEX("Use Vertex Colors", Float) = 0

//      // Foam
//      foam_color: vec4<f32>, // ("Foam Tint", Color) = (1,1,1,1)
//      foam: f32, // ("Edgefoam Width", Range(1,50)) = 2.35
//      foam_top_spread: f32, // ("Foam Position", Range(-1,6)) = 0.05
//      foam_softness: f32, // ("Foam Softness", Range(0,0.5)) = 0.1
//      edge_width: f32, // ("Foam Width", Range(0,2)) = 0.4

//      // Rim Light
//      rim_color: vec4<f32>, // ("Rim Color", Color) = (0,0.5,0.25,1)
//      rim_power: f32, // ("Rim Power", Range(1,20)) = 18

//      // Vertex Movement
//      wave_amount: f32, // ("Wave Amount", Range(0,10)) = 0.6
//      wave_speed: f32, // ("Speed", Range(0,10)) = 0.5
//      wave_height: f32, // ("Wave Height", Range(0,1)) = 0.1

//      // Reflections
//      reflectivity: f32, // ("Reflectivity", Range(0,1)) = 0.6
//  }

//      SideNoiseTex ("Side Water Texture", 2D) = "white" {}
//      TopNoiseTex ("Top Water Texture", 2D) = "white" {}
//      ReflectionTex("Refl Texture", 2D) = "black" {}

//  // Physically based Standard lighting model, and enable shadows on all light types
//  #pragma surface surf Standard vertex:vert fullforwardshadows keepalpha

//  // Use shader model 3.0 target, to get nicer looking lighting
//  #pragma target 3.0
//  #pragma shader_feature VERTEX


//  sampler2D _SideNoiseTex, _TopNoiseTex;
//  uniform sampler2D _CameraDepthTexture; //Depth Texture

//  struct Input {
//      world_normal: vec3<f32>,// world normal built-in value
//      world_pos: vec3<f32>,   // world position built-in value
//      view_dir: vec3<f32>,    // view direction for rim
//      color: vec4<f32>,       // vertex colors
//      screen_pos: vec4<f32>,  // screen position for edgefoam
//      eye_depth: f32,         // depth for edgefoam
//  }

//  float _SpeedV, _Amount, _Height;
//  fixed4 _FoamColor, _WaterColor, _RimColor, _TColor;
//  fixed _HorSpeed, _TopScale, _TopSpread, _EdgeWidth, _RimPower, _NoiseScale, _VertSpeed;
//  float _Brightness, _Foam, _Softness;
//  float _DepthOffset, _Stretch;
//  sampler2D _ReflectionTex;
//  float _Reflectivity;

//  void vert (inout appdata_full v, out Input o) {
//      UNITY_INITIALIZE_OUTPUT(Input, o);
//      COMPUTE_EYEDEPTH(o.eyeDepth);

//      let world_normal = mul(unity_ObjectToWorld, v.normal);
//      let world_pos = mul(unity_ObjectToWorld, v.vertex).xyz;
//      let tex = tex2Dlod(_SideNoiseTex, float4(worldPos.xz * _TopScale * 1, 1,1));
//      let movement = sin(_Time.z * _SpeedV + (v.vertex.x * v.vertex.z * _Amount * tex)) * _Height * (1 - worldNormal.y);
//      v.vertex.xyz += movement;
//  }


//  uniform float3 _Position;
//  uniform sampler2D _GlobalEffectRT;
//  uniform float _OrthographicCamSize;

//  fn surf(Input IN, inout SurfaceOutputStandard  o) {

//      // get the world normal
//      let world_normal = WorldNormalVector(IN, o.Normal);
//      // grab the vertex colors from the model
//      let vertex_colors = IN.color.rgb;
//      // normal for triplanar mapping
//      let blend_normal = saturate(pow(worldNormal * 1.4,4));


//      #if VERTEX // use vertex colors for flow
//      let flow_dir = (vertex_colors * 2.0) - 1.0;
//      #else // or world normal
//      let flow_dir = -(world_normal * 2.0) - 1.0;
//      #endif

//      // horizontal flow speed
//      let flow_dir = flow_dir * _HorSpeed;

//      // flowmap blend timings
//      let timing1 = frac(_Time[1] * 0.5 + 0.5);
//      let timing2 = frac(_Time[1] * 0.5);
//      let timing_lerp = abs((0.5 - timing1) / 0.5);

//      // move 2 textures at slight different speeds fased on the flowdirection
//      let top_tex1 = tex2D(_TopNoiseTex, (IN.worldPos.xz * _TopScale) + flowDir.xz * timing1);
//      let top_tex2 = tex2D(_TopNoiseTex, (IN.worldPos.xz * _TopScale) + flowDir.xz * timing2);

//      // vertical flow speed
//      let vert_flow = _Time.y * _VertSpeed;

//      // rendertexture UV
//      let uv = in.world_pos.xz - _Position.xz;
//      let uv = uv / (_OrthographicCamSize * 2) + vec2<f32>(0.5);

//      // Ripples
//      let ripples = tex2D(_GlobalEffectRT, uv).b;

//      // noise sides
//      let top_foam_noise = lerp(topTex1, topTex2, timingLerp) + ripples;

//      let side_foam_noise_z  = tex2d(side_noise_tex, vec2<f32>(world_pos.z * 10.0, world_pos.y + vert_flow) * params.noise_scale );
//      let side_foam_noise_x  = tex2d(side_noise_tex, vec2<f32>(world_pos.x * 10.0, world_pos.y + vert_flow) * params.noise_scale);
//      let side_foam_noise_ze = tex2d(side_noise_tex, vec2<f32>(world_pos.z * 10.0, world_pos.y + vert_flow) * params.noise_scale/3.0);
//      let side_foam_noise_xe = tex2d(side_noise_tex, vec2<f32>(world_pos.x * 10.0, world_pos.y + vert_flow) * params.noise_scale/3.0);

//      // lerped together all sides for noise texture
//      let noisetexture = (SideFoamNoiseX + SideFoamNoiseXE) /2;
//      let noisetexture = lerp(noisetexture, (SideFoamNoiseZ +SideFoamNoiseZE) / 2, blendNormal.x);
//      let noisetexture = lerp(noisetexture, TopFoamNoise, blendNormal.y);

//      // Normalbased Foam
//      let world_normal_dot_noise = noisetexture * dot(o.Normal, worldNormal.y + 0.3);

//      // add noise to normal
//      o.normal = BlendNormals(o.normal, noisetexture * 2);
//      o.normal = BlendNormals(o.normal, ripples * 2);

//      // edge foam calculation
//      let depth = LinearEyeDepth(SAMPLE_DEPTH_TEXTURE_PROJ(_CameraDepthTexture ,IN.screen_pos)  ); // depth
//      let foam_line_s = 1.0 - saturate(_Foam * vec4<f32>(noisetexture, 1.0) * (depth - IN.screen_pos.w)); // foam line by comparing depth and screenposition
//      let foam_line: vec4<f32> = smoothstep(0.5, 0.8, foam_line_s);

//      // rimline
//      let rim = i32(1.0 - saturate(dot(normalize(IN.viewDir) , o.Normal)));
//      let color_rim = _RimColor.rgb * pow(rim, _RimPower);

//      let foam_s = 4.0 * smoothstep(world_normal_dot_noise, world_normal_dot_noise + _Softness, _TopSpread + _EdgeWidth);
//      let foam_s = foam_s * saturate(1-worldNormal.y );

//      // combine depth foam and foam + add color
//      let combined_foam = (foam_s + foam_line.rgb + ripples) * _FoamColor;

//      // colors lerped over blendnormal
//      let color = lerp(_WaterColor, _TColor, saturate((depth - IN.screenPos.w + _DepthOffset * noisetexture.r) * _Stretch) ) * _Brightness;
//      o.albedo = color;

//      o.smoothness = smoothstep(0.0, 0.5, o.normal);
//      o.metallic = 0.5;

//      // glowing combined foam and colored rim

//      let rt_reflections = tex2Dproj(_ReflectionTex, UNITY_PROJ_COORD(IN.screenPos + worldNormalDotNoise)) * dot(o.Normal, worldNormal.y);
//      o.albedo += combinedFoam + colorRim + rtReflections;

//      // clamped alpha
//      o.alpha = saturate(color.a + (rt_reflections * _Reflectivity) + combined_foam + foam_line.a + ripples);
//  }
