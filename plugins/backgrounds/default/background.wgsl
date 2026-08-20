// Background-plugin ABI. This file owns the current comic theme; the native
// compositor only calls the function and knows nothing about its appearance.
fn plugin_background(uv: vec2f, time: f32) -> vec3f {
    let center = distance(uv, vec2f(0.5, 0.48));
    let pulse = 0.04 * sin(time * 0.8 + center * 18.0);
    let top = vec3f(0.035, 0.055, 0.11);
    let bottom = vec3f(0.22, 0.025, 0.07);
    var color = mix(top, bottom, clamp(uv.y + pulse, 0.0, 1.0));
    let grid = step(0.88, fract(uv.x * 80.0)) * step(0.88, fract(uv.y * 45.0));
    color += vec3f(0.12, 0.03, 0.08) * grid;
    return color;
}
