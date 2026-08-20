// A second built-in plugin kept intentionally simple: it demonstrates that
// changing a manifest changes the theme without modifying compositor code.
fn plugin_background(uv: vec2f, time: f32) -> vec3f {
    let glow = 0.035 * sin(time * 0.35 + uv.x * 5.0);
    let top = vec3f(0.025, 0.035, 0.055);
    let bottom = vec3f(0.075, 0.09, 0.13);
    let vignette = smoothstep(0.78, 0.22, distance(uv, vec2f(0.5, 0.48)));
    return mix(top, bottom, uv.y) + vec3f(glow) + vec3f(0.018, 0.022, 0.03) * vignette;
}
