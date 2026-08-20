fn jp_hash(p: vec2f) -> f32 {
    return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453);
}

fn jp_noise(p: vec2f) -> f32 {
    let cell = floor(p);
    let local = fract(p);
    let blend = local * local * (3.0 - 2.0 * local);
    let a = jp_hash(cell);
    let b = jp_hash(cell + vec2f(1.0, 0.0));
    let c = jp_hash(cell + vec2f(0.0, 1.0));
    let d = jp_hash(cell + vec2f(1.0, 1.0));
    return mix(mix(a, b, blend.x), mix(c, d, blend.x), blend.y);
}

fn jp_disc(uv: vec2f, center: vec2f, radius: f32, softness: f32) -> f32 {
    return 1.0 - smoothstep(radius, radius + softness, distance(uv, center));
}

fn jp_ellipse(uv: vec2f, center: vec2f, radius: vec2f, softness: f32) -> f32 {
    return 1.0 - smoothstep(1.0, 1.0 + softness, length((uv - center) / radius));
}

fn jp_box(uv: vec2f, center: vec2f, half_size: vec2f, radius: f32, softness: f32) -> f32 {
    let q = abs(uv - center) - half_size + vec2f(radius);
    let distance_to_box = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - radius;
    return 1.0 - smoothstep(0.0, softness, distance_to_box);
}

fn jp_line(uv: vec2f, start: vec2f, end: vec2f, width: f32) -> f32 {
    let segment = end - start;
    let t = clamp(dot(uv - start, segment) / max(dot(segment, segment), 0.00001), 0.0, 1.0);
    return 1.0 - smoothstep(width, width * 1.8, distance(uv, start + segment * t));
}

fn jp_cloud(uv: vec2f, center: vec2f, size: vec2f) -> f32 {
    var cloud = jp_ellipse(uv, center, size, 0.08);
    cloud = max(cloud, jp_ellipse(uv, center + vec2f(size.x * 0.45, -size.y * 0.18), size * vec2f(0.7, 0.84), 0.08));
    cloud = max(cloud, jp_ellipse(uv, center - vec2f(size.x * 0.5, size.y * 0.06), size * vec2f(0.66, 0.72), 0.08));
    return cloud;
}

fn jp_sakura(uv: vec2f, center: vec2f, size: f32) -> f32 {
    var crown = jp_disc(uv, center, size * 0.24, 0.004);
    crown = max(crown, jp_disc(uv, center + vec2f(-0.19, -0.05) * size, size * 0.18, 0.004));
    crown = max(crown, jp_disc(uv, center + vec2f(0.19, -0.04) * size, size * 0.20, 0.004));
    crown = max(crown, jp_disc(uv, center + vec2f(-0.10, -0.19) * size, size * 0.19, 0.004));
    crown = max(crown, jp_disc(uv, center + vec2f(0.10, -0.20) * size, size * 0.18, 0.004));
    return crown;
}

fn plugin_background(uv: vec2f, time: f32) -> vec3f {
    let horizon = 0.455;
    let sky_mix = smoothstep(0.02, horizon, uv.y);
    var color = mix(vec3f(0.10, 0.20, 0.37), vec3f(0.94, 0.55, 0.48), sky_mix);

    let sun_center = vec2f(0.79, 0.23);
    let sunset_glow = exp(-distance(uv, sun_center) * 7.0);
    color += vec3f(0.32, 0.20, 0.09) * sunset_glow;
    color = mix(color, vec3f(1.0, 0.82, 0.54), jp_disc(uv, sun_center, 0.048, 0.018));

    let drift = fract(time * 0.0005) * 0.035;
    var clouds = jp_cloud(uv, vec2f(0.25 + drift, 0.19), vec2f(0.13, 0.026));
    clouds = max(clouds, jp_cloud(uv, vec2f(0.61 - drift, 0.31), vec2f(0.10, 0.020)));
    color = mix(color, vec3f(0.95, 0.67, 0.67), clouds * 0.34);

    let mountain_y = 0.405 + 0.035 * sin(uv.x * 8.0 + 0.8) + 0.013 * sin(uv.x * 23.0);
    color = mix(color, vec3f(0.24, 0.30, 0.43), smoothstep(mountain_y - 0.004, mountain_y + 0.004, uv.y));
    let tree_line = 0.435 + 0.008 * sin(uv.x * 71.0) + 0.006 * jp_noise(vec2f(uv.x * 90.0, 4.0));
    color = mix(color, vec3f(0.10, 0.20, 0.22), smoothstep(tree_line - 0.003, tree_line + 0.003, uv.y));

    let platform = smoothstep(horizon - 0.004, horizon + 0.006, uv.y);
    let platform_noise = (jp_noise(uv * vec2f(145.0, 90.0)) - 0.5) * 0.035;
    let platform_color = vec3f(0.62, 0.57, 0.51) + vec3f(platform_noise);
    color = mix(color, platform_color, platform);

    let track_edge = 0.59 + (uv.y - horizon) * 0.43;
    let track_mask = smoothstep(track_edge - 0.006, track_edge + 0.006, uv.x) * platform;
    let gravel = vec3f(0.20, 0.22, 0.25) + vec3f(0.07) * jp_noise(uv * vec2f(180.0, 110.0));
    color = mix(color, gravel, track_mask);

    let vanish = vec2f(0.60, horizon);
    let left_rail = jp_line(uv, vanish, vec2f(0.73, 1.03), 0.0045);
    let right_rail = jp_line(uv, vanish + vec2f(0.025, 0.0), vec2f(0.92, 1.03), 0.0055);
    let rail_highlight = max(left_rail, right_rail) * track_mask;
    color = mix(color, vec3f(0.68, 0.67, 0.64), rail_highlight);

    for (var sleeper_index = 0; sleeper_index < 11; sleeper_index = sleeper_index + 1) {
        let number = f32(sleeper_index) / 10.0;
        let depth = number * number;
        let y = horizon + 0.035 + depth * 0.53;
        let left_x = 0.60 + (y - horizon) * 0.22;
        let right_x = 0.64 + (y - horizon) * 0.52;
        let sleeper = jp_line(uv, vec2f(left_x, y), vec2f(right_x, y + 0.004), 0.003 + depth * 0.006);
        color = mix(color, vec3f(0.28, 0.20, 0.17), sleeper * track_mask * 0.9);
    }

    let warning_edge = jp_line(uv, vec2f(0.585, horizon), vec2f(0.82, 1.02), 0.011) * platform;
    color = mix(color, vec3f(0.92, 0.69, 0.20), warning_edge);
    let warning_inner = jp_line(uv, vec2f(0.585, horizon), vec2f(0.82, 1.02), 0.0025) * platform;
    color = mix(color, vec3f(0.99, 0.84, 0.35), warning_inner);

    for (var tile_index = 1; tile_index < 7; tile_index = tile_index + 1) {
        let number = f32(tile_index);
        let y = horizon + 0.018 * number * number;
        let tile_line = (1.0 - smoothstep(0.0015, 0.0035, abs(uv.y - y))) * platform * (1.0 - track_mask);
        color = mix(color, vec3f(0.49, 0.46, 0.43), tile_line * 0.45);
    }

    let canopy_top = 0.105 + uv.x * 0.17;
    let canopy_bottom = canopy_top + 0.095;
    let canopy_x = 1.0 - smoothstep(0.61, 0.625, uv.x);
    let canopy_band = smoothstep(canopy_top - 0.006, canopy_top + 0.004, uv.y)
        * (1.0 - smoothstep(canopy_bottom - 0.004, canopy_bottom + 0.006, uv.y)) * canopy_x;
    color = mix(color, vec3f(0.075, 0.15, 0.19), canopy_band);
    let canopy_edge = (1.0 - smoothstep(0.0, 0.009, abs(uv.y - canopy_bottom))) * canopy_x;
    color = mix(color, vec3f(0.24, 0.33, 0.35), canopy_edge);
    let warm_under_roof = smoothstep(canopy_bottom, canopy_bottom + 0.12, uv.y)
        * (1.0 - smoothstep(0.48, 0.56, uv.y)) * canopy_x;
    color += vec3f(0.055, 0.038, 0.016) * warm_under_roof;

    for (var column_index = 0; column_index < 3; column_index = column_index + 1) {
        let number = f32(column_index);
        let x = 0.095 + number * 0.205;
        let top_y = 0.205 + x * 0.17;
        let column = jp_line(uv, vec2f(x, top_y), vec2f(x - 0.018, 0.96), 0.0065);
        color = mix(color, vec3f(0.17, 0.24, 0.25), column);
        let lamp = jp_box(uv, vec2f(x + 0.052, top_y + 0.055), vec2f(0.023, 0.012), 0.006, 0.003);
        let lamp_glow = jp_ellipse(uv, vec2f(x + 0.052, top_y + 0.06), vec2f(0.07, 0.055), 0.2);
        color += vec3f(0.10, 0.065, 0.018) * lamp_glow;
        color = mix(color, vec3f(1.0, 0.78, 0.36), lamp);
    }

    let sign_post = jp_box(uv, vec2f(0.175, 0.52), vec2f(0.006, 0.115), 0.002, 0.002);
    color = mix(color, vec3f(0.20, 0.25, 0.26), sign_post);
    let station_sign = jp_box(uv, vec2f(0.175, 0.435), vec2f(0.092, 0.036), 0.007, 0.003);
    color = mix(color, vec3f(0.93, 0.92, 0.84), station_sign);
    let sign_stripe = jp_box(uv, vec2f(0.175, 0.454), vec2f(0.084, 0.006), 0.002, 0.002);
    color = mix(color, vec3f(0.18, 0.52, 0.54), sign_stripe);
    let sign_marks = max(
        jp_box(uv, vec2f(0.15, 0.427), vec2f(0.022, 0.004), 0.001, 0.001),
        jp_box(uv, vec2f(0.205, 0.427), vec2f(0.017, 0.004), 0.001, 0.001),
    );
    color = mix(color, vec3f(0.28, 0.31, 0.31), sign_marks * station_sign);

    let tree_trunk = jp_line(uv, vec2f(0.90, 0.47), vec2f(0.90, 0.64), 0.012);
    color = mix(color, vec3f(0.25, 0.16, 0.16), tree_trunk);
    let blossom = jp_sakura(uv, vec2f(0.90, 0.42), 0.34);
    let blossom_detail = jp_noise(uv * 95.0);
    color = mix(color, mix(vec3f(0.88, 0.48, 0.56), vec3f(1.0, 0.74, 0.77), blossom_detail), blossom * 0.94);

    let bench_seat = jp_box(uv, vec2f(0.34, 0.63), vec2f(0.10, 0.015), 0.004, 0.003);
    let bench_leg_left = jp_box(uv, vec2f(0.29, 0.68), vec2f(0.009, 0.05), 0.002, 0.002);
    let bench_leg_right = jp_box(uv, vec2f(0.39, 0.68), vec2f(0.009, 0.05), 0.002, 0.002);
    color = mix(color, vec3f(0.19, 0.32, 0.34), max(bench_seat, max(bench_leg_left, bench_leg_right)));

    let vignette = smoothstep(0.84, 0.30, distance(uv, vec2f(0.5, 0.48)));
    color *= 0.82 + vignette * 0.18;
    return color;
}
