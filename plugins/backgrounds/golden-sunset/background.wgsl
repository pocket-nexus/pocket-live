fn golden_hash(p: vec2f) -> f32 {
    return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453);
}

fn golden_noise(p: vec2f) -> f32 {
    let cell = floor(p);
    let local = fract(p);
    let blend = local * local * (3.0 - 2.0 * local);
    let a = golden_hash(cell);
    let b = golden_hash(cell + vec2f(1.0, 0.0));
    let c = golden_hash(cell + vec2f(0.0, 1.0));
    let d = golden_hash(cell + vec2f(1.0, 1.0));
    return mix(mix(a, b, blend.x), mix(c, d, blend.x), blend.y);
}

fn golden_fbm(p: vec2f) -> f32 {
    var value = 0.0;
    var amplitude = 0.55;
    var point = p;
    for (var octave = 0; octave < 4; octave = octave + 1) {
        value += golden_noise(point) * amplitude;
        point = point * 2.03 + vec2f(17.3, 9.1);
        amplitude *= 0.5;
    }
    return value;
}

fn golden_disc(uv: vec2f, center: vec2f, radius: f32, softness: f32) -> f32 {
    return 1.0 - smoothstep(radius, radius + softness, distance(uv, center));
}

fn golden_cloud_blob(uv: vec2f, center: vec2f, size: vec2f) -> f32 {
    let point = (uv - center) / size;
    return 1.0 - smoothstep(0.82, 1.0, length(point));
}

fn golden_tree(uv: vec2f, center_x: f32, base_y: f32, size: f32) -> f32 {
    let trunk = (1.0 - smoothstep(size * 0.055, size * 0.075, abs(uv.x - center_x)))
        * smoothstep(base_y, base_y - size * 0.06, uv.y)
        * smoothstep(base_y - size * 0.42, base_y - size * 0.36, uv.y);
    var crown = golden_disc(uv, vec2f(center_x, base_y - size * 0.43), size * 0.13, 0.004);
    crown = max(crown, golden_disc(uv, vec2f(center_x - size * 0.09, base_y - size * 0.34), size * 0.11, 0.004));
    crown = max(crown, golden_disc(uv, vec2f(center_x + size * 0.09, base_y - size * 0.34), size * 0.11, 0.004));
    crown = max(crown, golden_disc(uv, vec2f(center_x, base_y - size * 0.25), size * 0.12, 0.004));
    return max(trunk * 0.65, crown);
}

fn plugin_background(uv: vec2f, time: f32) -> vec3f {
    let horizon = 0.62;
    let sky_t = clamp(uv.y / horizon, 0.0, 1.0);
    let top = vec3f(0.42, 0.20, 0.48);
    let rose = vec3f(0.92, 0.43, 0.38);
    let gold = vec3f(1.0, 0.76, 0.34);
    var color = mix(mix(top, rose, smoothstep(0.0, 0.72, sky_t)), gold, smoothstep(0.58, 1.0, sky_t));

    let sun_center = vec2f(0.77, 0.21);
    let sun_distance = distance(uv, sun_center);
    let glow = exp(-sun_distance * 7.5);
    color += vec3f(0.42, 0.27, 0.08) * glow;
    color = mix(color, vec3f(1.0, 0.91, 0.55), golden_disc(uv, sun_center, 0.072, 0.018));

    let cloud_point = vec2f(uv.x * 4.2 + time * 0.006, uv.y * 11.0);
    let cloud_noise = golden_fbm(cloud_point);
    let cloud_band = exp(-pow((uv.y - 0.27) * 7.5, 2.0)) + 0.65 * exp(-pow((uv.y - 0.43) * 9.0, 2.0));
    let cloud = smoothstep(0.56, 0.78, cloud_noise * cloud_band + 0.30);
    color = mix(color, vec3f(1.0, 0.56, 0.40), cloud * 0.52);
    let drift = fract(time * 0.0007) * 0.03;
    var painted_cloud = golden_cloud_blob(uv, vec2f(0.17 + drift, 0.20), vec2f(0.15, 0.035));
    painted_cloud = max(painted_cloud, golden_cloud_blob(uv, vec2f(0.26 + drift, 0.18), vec2f(0.11, 0.05)));
    painted_cloud = max(painted_cloud, golden_cloud_blob(uv, vec2f(0.50 - drift, 0.34), vec2f(0.14, 0.032)));
    painted_cloud = max(painted_cloud, golden_cloud_blob(uv, vec2f(0.58 - drift, 0.32), vec2f(0.09, 0.045)));
    color = mix(color, vec3f(0.96, 0.48, 0.43), painted_cloud * 0.38);

    let far_hill = 0.52 + 0.055 * sin(uv.x * 7.0 + 0.7) + 0.025 * sin(uv.x * 19.0);
    color = mix(color, vec3f(0.38, 0.19, 0.50), smoothstep(far_hill - 0.006, far_hill + 0.006, uv.y));
    let near_hill = 0.59 + 0.045 * sin(uv.x * 9.0 - 1.4) + 0.018 * sin(uv.x * 23.0 + 0.5);
    color = mix(color, vec3f(0.29, 0.15, 0.40), smoothstep(near_hill - 0.006, near_hill + 0.006, uv.y));

    let ground = smoothstep(horizon - 0.008, horizon + 0.008, uv.y);
    let ground_noise = golden_noise(uv * vec2f(85.0, 42.0));
    let ground_color = vec3f(0.82, 0.43, 0.28) + vec3f(0.07, 0.035, 0.015) * ground_noise;
    color = mix(color, ground_color, ground);

    for (var index = 0; index < 9; index = index + 1) {
        if index == 0 || index == 2 || index == 5 || index == 8 {
        let number = f32(index);
        let random = golden_hash(vec2f(number, 3.7));
        let center_x = (number + 0.18 + random * 0.64) / 9.0;
        let size = 0.15 + random * 0.10;
        let base_y = horizon + 0.025 + 0.025 * golden_hash(vec2f(number, 8.1));
        let tree = golden_tree(uv, center_x, base_y, size);
        let tree_color = mix(vec3f(0.17, 0.19, 0.11), vec3f(0.31, 0.28, 0.12), random);
        color = mix(color, tree_color, tree);
        }
    }

    let vignette = smoothstep(0.82, 0.30, distance(uv, vec2f(0.5, 0.48)));
    color *= 0.84 + vignette * 0.16;
    return color;
}
