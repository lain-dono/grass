struct Params {
    // Stars Settings
    stars_cutoff: f32, //= 0.08, // 0..1
    stars_speed: f32, //= 0.3 // 0..1
    stars_sky_color: vec4<f32>, //=(0.0,0.2,0.1,1)

    // Horizon Settings
    offset_horizon: f32, //=0 -1..1
    horizon_intensity: f32, //("Horizon Intensity",  Range(0, 10)) = 3.3
    sun_set: vec4<f32>, //("Sunset/Rise Color", Color) = (1,0.8,1,1)
    horizon_color_day: vec4<f32>, // ("Day Horizon Color", Color) = (0,0.8,1,1)
    horizon_color_night: vec4<f32>, // ("Night Horizon Color", Color) = (0,0.8,1,1)

    // Sun Settings
    sun_color: vec4<f32>, // ("Sun Color", Color) = (1,1,1,1)
    sun_radius: f32, // ("Sun Radius",  Range(0, 2)) = 0.1

    // Moon Settings
    moon_color: vec4<f32>, // ("Moon Color", Color) = (1,1,1,1)
    moon_radius: f32, // ("Moon Radius",  Range(0, 2)) = 0.15
    moon_offset: f32, // ("Moon Crescent",  Range(-1, 1)) = -0.1

    // Day Sky Settings
    day_top_color: vec4<f32>, // ("Day Sky Color Top", Color) = (0.4,1,1,1)
    day_bottom_color: vec4<f32>, // ("Day Sky Color Bottom", Color) = (0,0.8,1,1)

    // Main Cloud Settings
    base_noise_scale: f32, // ("Base Noise Scale",  Range(0, 1)) = 0.2
    distort_scale: f32, // ("Distort Noise Scale",  Range(0, 1)) = 0.06
    seccondary_noise_scale: f32, // ("Secondary Noise Scale",  Range(0, 1)) = 0.05
    extra_distortion: f32, // ("Extra Distortion",  Range(0, 1)) = 0.1
    movement_speed: f32, // ("Movement Speed",  Range(0, 10)) = 1.4
    cloud_cutoff: f32, // ("Cloud Cutoff",  Range(0, 1)) = 0.3
    fuzziness: f32, // ("Cloud Fuzziness",  Range(0, 1)) = 0.04
    fuzziness_under: f32, // ("Cloud Fuzziness Under",  Range(0, 1)) = 0.01

    //[Toggle(FUZZY)] _FUZZY("Extra Fuzzy clouds", Float) = 1

    // Day Clouds Settings
    cloud_color_day_edge: vec4<f32>, // ("Clouds Edge Day", Color) = (1,1,1,1)
    cloud_color_day_main: vec4<f32>, // ("Clouds Main Day", Color) = (0.8,0.9,0.8,1)
    cloud_color_day_under: vec4<f32>, // ("Clouds Under Day", Color) = (0.6,0.7,0.6,1)
    cloud_brightness: f32, // ("Cloud Brightness",  Range(1, 10)) = 2.5

    // Night Sky Settings
    night_top_color: vec4<f32>, // ("Night Sky Color Top", Color) = (0,0,0,1)
    night_bottom_color: vec4<f32>, // ("Night Sky Color Bottom", Color) = (0,0,0.2,1)

    // Night Clouds Settings
    cloud_color_night_edge: vec4<f32>, // ("Clouds Edge Night", Color) = (0,1,1,1)
    cloud_color_night_main: vec4<f32>, // ("Clouds Main Night", Color) = (0,0.2,0.8,1)
    cloud_color_night_under: vec4<f32>, // ("Clouds Under Night", Color) = (0,0.2,0.6,1)
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var any_sampler: sampler;
@group(0) @binding(2) var stars_texture: texture_2d<f32>;
@group(0) @binding(3) var base_noise: texture_2d<f32>;
@group(0) @binding(4) var distort: texture_2d<f32>;
@group(0) @binding(5) var secondary_noise: texture_2d<f32>;


    struct appdata
    {
        float4 vertex : POSITION;
        float3 uv : TEXCOORD0;
    };

    struct v2f
    {
        float3 uv : TEXCOORD0;
        UNITY_FOG_COORDS(1)
        float4 vertex : SV_POSITION;
        float3 worldPos : TEXCOORD1;
    };

    sampler2D _Stars, _BaseNoise, _Distort, _SecNoise;
v2f vert(appdata v)
{
    v2f o;
    o.vertex = UnityObjectToClipPos(v.vertex);
    o.uv = v.uv;
    o.worldPos = mul(unity_ObjectToWorld, v.vertex);
    UNITY_TRANSFER_FOG(o,o.vertex);
    return o;
}

fn frag(v2f i) : SV_Target {
    let horizon = abs((i.uv.y * _HorizonIntensity) - _OffsetHorizon);

    // uv for the sky
    let sky_uv = i.world_pos.xz / i.world_pos.y;

    // moving clouds
    let base_noise = tex2D(_BaseNoise, (sky_uv - _Time.x) * _BaseNoiseScale).x;
    let noise1 = tex2D(_Distort, ((sky_uv + base_noise) - (_Time.x * _Speed)) * _DistortScale);
    let noise2 = tex2D(_SecNoise, ((sky_uv + (noise1 * _Distortion)) - (_Time.x * (_Speed * 0.5))) * _SecNoiseScale);
    let final_noise = saturate(noise1 * noise2) * 3 * saturate(i.worldPos.y);

    // if fuzzy reset base_noise to 1

    let noised_cutoff = _CloudCutoff * base_noise;

    let clouds      = saturate(smoothstep(noised_cutoff, noised_cutoff + _Fuzziness, final_noise));
    let cloudsunder = saturate(smoothstep(noised_cutoff, noised_cutoff + _Fuzziness + _FuzzinessUnder, noise2) * clouds);

    var clouds_colored       = lerp(_CloudColorDayEdge,   lerp(_CloudColorDayUnder,   _CloudColorDayMain,   cloudsunder), clouds) * clouds;
    var clouds_colored_night = lerp(_CloudColorNightEdge, lerp(_CloudColorNightUnder, _CloudColorNightMain, cloudsunder), clouds) * clouds;
    clouds_colored_night *= horizon;

    clouds_colored = lerp(clouds_colored_night, clouds_colored, saturate(_WorldSpaceLightPos0.y)); // lerp the night and day clouds over the light direction
    clouds_colored += (_Brightness * clouds_colored * horizon); // add some extra brightness

    let clouds_negative = (1 - clouds) * horizon;

    // sun
    let sun = distance(i.uv.xyz, _WorldSpaceLightPos0);
    let sun_disc = 1 - (sun / _SunRadius);
    let sun_disc = saturate(sunDisc * 50);

    // (crescent) moon
    let moon = distance(i.uv.xyz, -_WorldSpaceLightPos0);
    let crescent_moon = distance(float3(i.uv.x + _MoonOffset, i.uv.yz), -_WorldSpaceLightPos0);
    let crescent_moon_disc = 1 - (crescent_moon / _MoonRadius);
    let crescent_moon_disc = saturate(crescent_moon_disc * 50.0);
    let moon_disc = 1 - (moon / _MoonRadius);
    let moon_disc = saturate(moon_disc * 50.0);
    let moon_disc = saturate(moon_disc - crescent_moon_disc);

    let sun_and_moon = (sun_disc * _SunColor + moon_disc * _MoonColor) * clouds_negative;

    // stars
    var stars = tex2D(_Stars, sky_uv + _StarsSpeed * _Time.x);
    stars *= saturate(-_WorldSpaceLightPos0.y);
    stars = step(_StarsCutoff, stars);
    stars += base_noise * _StarsSkyColor;
    stars *= clouds_negative;

    let gradient_day   = lerp(_DayBottomColor, _DayTopColor, saturate(horizon));
    let gradient_night = lerp(_NightBottomColor, _NightTopColor, saturate(horizon));
    let sky_gradients  = lerp(gradient_night, gradient_day, saturate(_WorldSpaceLightPos0.y)) * clouds_negative;

    // horizon glow / sunset/rise
    let sunset = _SunSet * saturate((1 - horizon) * saturate(_WorldSpaceLightPos0.y * 5));

    let horizon_glow_night = saturate((1.0 - horizon * 5.0) * saturate(-_WorldSpaceLightPos0.y * 10)) * _HorizonColorNight;
    let horizon_glow       = saturate((1.0 - horizon * 5.0) * saturate( _WorldSpaceLightPos0.y * 10)) * _HorizonColorDay + horizon_glow_night;


    let combined = sky_gradients + sun_and_moon + sunset + stars + clouds_colored + horizon_glow;

    // apply fog
    //UNITY_APPLY_FOG(i.fogCoord, combined);

    return vec4<f32>(combined, 1.0);
}