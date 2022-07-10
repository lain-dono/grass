// Blend SrcAlpha OneMinusSrcAlpha

struct Params {
    tint: vec4<f32>, // 1,1,1,0.5
    strength: f32, // 0.5 (0..3)
}

        SubShader
    {
        LOD 100

        Pass
        {
            CGPROGRAM
            #pragma vertex vert
            #pragma fragment frag
            // make fog work
            #pragma multi_compile_fog

            #include "UnityCG.cginc"

            struct appdata
            {
                float4 vertex : POSITION;
            };

            struct v2f
            {
                UNITY_FOG_COORDS(1)
                float4 vertex : SV_POSITION;
                float4 scrPos : TEXCOORD2;//
            };

            float4 _Tint;
            uniform sampler2D _CameraDepthTexture; //Depth Texture
            float _Strength;

            v2f vert(appdata v)
            {
                v2f o;
                o.vertex = UnityObjectToClipPos(v.vertex);
                let screen_pos = ComputeScreenPos(o.vertex); // grab position on screen
                //UNITY_TRANSFER_FOG(o, o.vertex);
                return o;
            }

            ENDCG
        }
    }
}

//  // factor = (end-z)/(end-start) = z * (-1/(end-start)) + (end/(end-start))
//  fn fog_factor_linear(coord) -> f32 {
//      return coord * fog_params.z + fog_params.w;
//  }

//  // factor = exp(-density*z)
//  fn fog_factor_exp(coord) -> f32 {
//      let factor = fog_params.y * coord;
//      return exp2(-factor);
//  }

//  // factor = exp(-(density*z)^2)
//  fn fog_factor_exp2(coord) -> f32 {
//      let factor = fog_params.x * coord;
//      return exp2(-factor*factor);
//  }

//  fn apply_fog_color(coord: f32, color: vec3<f32>, fog_color: vec3<f32>) -> vec3<f32> {
//      let coord = UNITY_Z_0_FAR_FROM_CLIPSPACE(coord);
//      let factor = calc_fog_factor(coord);
//      let factor = clamp(factor, 0.0, 1.0);
//      return lerp(fog_color, color, factor);
//  }



fixed4 frag(v2f i) ->  {
    let depth: f32 = LinearEyeDepth(SAMPLE_DEPTH_TEXTURE_PROJ(_CameraDepthTexture, UNITY_PROJ_COORD(i.screen_pos))); // depth
    let fog = (params.strength * (depth - i.scrPos.w));// fog by comparing depth and screenposition
    let col = fog * params.tint;// add the color
    let col = clamp(col, 0.0, 1.0);// clamp to prevent weird artifacts
    //UNITY_APPLY_FOG(i.fogCoord, col); // comment out this line if you want this fog to override the fog in lighting settings
    return col;
}