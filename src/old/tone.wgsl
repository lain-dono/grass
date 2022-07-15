// variable HDR GT Tonemap,
// as described in http://cdn2.gran-turismo.com/data/www/pdi_publications/PracticalHDRandWCGinGTS_20181222.pdf
// https://www.desmos.com/calculator/gslcdxvipg

// read the PDF about this if you precompute it! it's got helpful info!

// can be optimized into lut (compute can gen it)
fn gt_tonemap_item(x: f32) -> f32 {
    let m = 0.22; // linear section start
    let a = 1.0;  // contrast
    let c = 1.33; // black brightness
    let P = 1.0;  // maximum brightness
    let l = 0.4;  // linear section length

    let l0 = ((P-m)*l) / a; // 0.312
    let S0 = m + l0; // 0.532
    let S1 = m + a * l0; // 0.532
    let C2 = (a*P) / (P - S1); // 2.13675213675

    let L = m + a * (x - m);
    let T = m * pow(x/m, c);
    let S = P - (P - S1) * exp(-C2*(x - S0)/P);

    let w0 = 1 - smoothstep(0.0, m, x);
    let w2 = select(1, 0, x < m+l);
    let w1 = 1 - w0 - w2;

    return f32(T * w0 + L * w1 + S * w2);
}

// this costs about 0.2-0.3ms more than aces, as-is
fn gt_tonemap(x: vec3<f32>) -> f32 {
    return vec3<f32>(
        gt_tonemap_item(x.r),
        gt_tonemap_item(x.g),
        gt_tonemap_item(x.b)
    );
}