//! pocket-character: the airi-parity character widget on the Pocket runtime.
//!
//! Windowed mode is the product: a transparent, undecorated, always-on-top
//! 450×600 window (airi's stage geometry) rendering the VRM character.
//! `--headless-shot` drives the same [`Game`] object without a window and
//! saves an RGBA screenshot — CI-friendly parity checks.

mod compositor;
mod frame_share;
mod guest;
mod tracking;
mod widget;

use std::path::PathBuf;

use anyhow::{Context, Result};
use pocket3d::app::{AppConfig, Game};
use pocket3d::gpu::{Gpu, OffscreenTarget};
use pocket3d::input::Input;
use pocket3d::renderer::Renderer;

use compositor::{BackgroundMode, CompositorConfig};
use tracking::{FaceLaunch, VisionLaunch};
use widget::{Widget, WidgetConfig};

const WIDGET_SIZE: (u32, u32) = (450, 600);
const TICK_HZ: f32 = 60.0;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| -> Option<String> {
        args.iter()
            .rposition(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let root = std::env::var("POCKET_CHARACTER_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let size = flag("--output-size")
        .as_deref()
        .map(parse_size)
        .transpose()?
        .unwrap_or(WIDGET_SIZE);
    let background = flag("--background")
        .as_deref()
        .map(|value| {
            BackgroundMode::parse(value).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown --background mode '{value}'; expected transparent, virtual, camera, matte, or clean"
                )
            })
        })
        .transpose()?
        .unwrap_or(BackgroundMode::Transparent);
    let clean_plate_delay = flag("--clean-plate-delay")
        .map(|value| value.parse::<f32>())
        .transpose()
        .context("--clean-plate-delay must be seconds")?
        .unwrap_or(3.0);

    let cfg = WidgetConfig {
        model_path: flag("--model")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("assets/AvatarSample_A.vrm")),
        vrma_path: flag("--vrma")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("assets/idle_loop.vrma")),
        bundle_path: flag("--bundle")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("dist/character.js")),
        size,
        frames: flag("--frames").and_then(|s| s.parse().ok()),
        frame_warmup: flag("--frame-warmup")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        vision: match flag("--tracking").as_deref() {
            Some("camera") => Some(VisionLaunch::camera(
                flag("--vision-bridge")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        root.join("native/PocketVisionBridge/.build/release/pocket-vision-bridge")
                    }),
                flag("--device"),
                Some(FaceLaunch {
                    python: flag("--face-python")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| root.join(".venv/bin/python")),
                    script: flag("--face-bridge")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| root.join("native/mediapipe_face_bridge.py")),
                    model: flag("--face-model")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| root.join("assets/face_landmarker.task")),
                    pose_model: flag("--pose-model")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| root.join("assets/pose_landmarker_lite.task")),
                    hand_model: flag("--hand-model")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| root.join("assets/hand_landmarker.task")),
                }),
            )),
            Some("mock") => Some(VisionLaunch::mock(
                flag("--vision-bridge")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        root.join("native/PocketVisionBridge/.build/release/pocket-vision-bridge")
                    }),
            )),
            Some(other) if other != "off" => {
                anyhow::bail!("unknown --tracking mode '{other}'; expected off, mock, or camera")
            }
            _ => None,
        },
        compositor: (background != BackgroundMode::Transparent).then_some(CompositorConfig {
            mode: background,
            clean_plate_delay: std::time::Duration::from_secs_f32(clean_plate_delay.max(0.0)),
        }),
    };

    if let Some(out) = flag("--headless-shot") {
        let ticks: u32 = flag("--ticks").and_then(|s| s.parse().ok()).unwrap_or(60);
        return headless_shot(cfg, ticks, PathBuf::from(out));
    }
    if let Some(dir) = flag("--headless-seq") {
        let ticks: u32 = flag("--ticks").and_then(|s| s.parse().ok()).unwrap_or(300);
        let skip: u32 = flag("--skip").and_then(|s| s.parse().ok()).unwrap_or(0);
        return headless_seq(cfg, ticks, skip, PathBuf::from(dir));
    }
    if let Some(frames) = flag("--headless-bench") {
        let frames: u32 = frames
            .parse()
            .context("--headless-bench must be a frame count")?;
        anyhow::ensure!(frames > 0, "benchmark frame count cannot be zero");
        return headless_benchmark(cfg, frames);
    }

    let max_fps = flag("--max-fps")
        .and_then(|s| s.parse().ok())
        .unwrap_or(60.0);
    let window_size = cfg.size;
    let transparent = cfg.compositor.is_none();
    let widget = Widget::new(cfg);
    let run = if transparent {
        pocket3d::app::run
    } else {
        pocket3d::app::run_physical
    };
    run(
        AppConfig {
            title: "pocket-character".into(),
            size: window_size,
            tick_hz: TICK_HZ,
            capture_mouse: false,
            transparent,
            decorations: false,
            always_on_top: true,
            resizable: false,
            max_fps: Some(max_fps),
            drag_window: true,
        },
        widget,
    )
}

fn parse_size(value: &str) -> Result<(u32, u32)> {
    let (width, height) = value
        .split_once('x')
        .or_else(|| value.split_once('X'))
        .ok_or_else(|| anyhow::anyhow!("--output-size must be WIDTHxHEIGHT"))?;
    let size = (width.parse::<u32>()?, height.parse::<u32>()?);
    anyhow::ensure!(size.0 > 0 && size.1 > 0, "output size cannot be zero");
    anyhow::ensure!(
        size.0 <= 7680 && size.1 <= 4320,
        "output size exceeds the 8K safety limit"
    );
    Ok(size)
}

/// Like `headless_shot`, but renders EVERY tick after `skip` into
/// `dir/frame-%05d.png` — filmstrips and videos for docs come from this.
fn headless_seq(cfg: WidgetConfig, ticks: u32, skip: u32, dir: PathBuf) -> Result<()> {
    let size = cfg.size;
    let gpu = Gpu::new_headless()?;
    let mut renderer = Renderer::new(&gpu, pocket3d::gpu::OFFSCREEN_FORMAT)?;
    let mut widget = Widget::new(cfg);
    widget.init(&gpu, &mut renderer)?;
    std::fs::create_dir_all(&dir)?;

    let input = Input::default();
    let dt = 1.0 / TICK_HZ;
    let target = OffscreenTarget::new(&gpu, size.0, size.1);
    for i in 0..(skip + ticks) {
        widget.frame(dt, &input);
        widget.tick(dt, &input);
        if i < skip {
            continue;
        }
        let (scene, camera, hud) = widget.compose(0.0, i as f32 * dt, size);
        renderer.render(&gpu, &target.view, size, scene, camera, hud);
        render_overlay(&mut widget, &gpu, &target, i as f32 * dt);
        target.save_png(&gpu, &dir.join(format!("frame-{:05}.png", i - skip)))?;
    }
    println!("wrote {} frames to {}", ticks, dir.display());
    Ok(())
}

/// Drive the widget for `ticks` fixed steps without a window, render one
/// frame offscreen, save it (alpha preserved — the transparent background
/// stays transparent in the PNG).
fn headless_shot(cfg: WidgetConfig, ticks: u32, out: PathBuf) -> Result<()> {
    let size = cfg.size;
    let gpu = Gpu::new_headless()?;
    let mut renderer = Renderer::new(&gpu, pocket3d::gpu::OFFSCREEN_FORMAT)?;
    let mut widget = Widget::new(cfg);
    widget.init(&gpu, &mut renderer)?;

    let input = Input::default();
    let dt = 1.0 / TICK_HZ;
    for _ in 0..ticks {
        widget.frame(dt, &input);
        widget.tick(dt, &input);
    }
    let (scene, camera, hud) = widget.compose(0.0, ticks as f32 * dt, size);
    let target = OffscreenTarget::new(&gpu, size.0, size.1);
    renderer.render(&gpu, &target.view, size, scene, camera, hud);
    render_overlay(&mut widget, &gpu, &target, ticks as f32 * dt);
    target.save_png(&gpu, &out)?;
    println!("wrote {}", out.display());
    Ok(())
}

/// Measure the complete deterministic host path at the requested output
/// size: simulation, pose injection, Pocket render and compositor. The GPU
/// is explicitly drained before timing ends, so this is not merely command
/// submission throughput.
fn headless_benchmark(cfg: WidgetConfig, frames: u32) -> Result<()> {
    let size = cfg.size;
    let gpu = Gpu::new_headless()?;
    let mut renderer = Renderer::new(&gpu, pocket3d::gpu::OFFSCREEN_FORMAT)?;
    let mut widget = Widget::new(cfg);
    widget.init(&gpu, &mut renderer)?;
    let input = Input::default();
    let dt = 1.0 / TICK_HZ;
    let target = OffscreenTarget::new(&gpu, size.0, size.1);

    for index in 0..30 {
        render_benchmark_frame(&mut widget, &mut renderer, &gpu, &target, &input, dt, index);
    }
    wait_for_gpu(&gpu)?;

    let started = std::time::Instant::now();
    for index in 0..frames {
        render_benchmark_frame(
            &mut widget,
            &mut renderer,
            &gpu,
            &target,
            &input,
            dt,
            index + 30,
        );
    }
    wait_for_gpu(&gpu)?;
    let seconds = started.elapsed().as_secs_f64();
    let throughput_fps = frames as f64 / seconds;
    let result = serde_json::json!({
        "schema_version": 1,
        "adapter": gpu.adapter.get_info().name,
        "width": size.0,
        "height": size.1,
        "frames": frames,
        "seconds": seconds,
        "throughput_fps": throughput_fps,
        "average_frame_ms": seconds * 1000.0 / frames as f64,
        "meets_60_fps": throughput_fps >= 60.0,
    });
    println!("BENCH {result}");
    Ok(())
}

fn wait_for_gpu(gpu: &Gpu) -> Result<()> {
    let (sender, receiver) = std::sync::mpsc::channel();
    gpu.queue.on_submitted_work_done(move || {
        let _ = sender.send(());
    });
    gpu.device.poll(wgpu::PollType::Wait)?;
    receiver
        .recv()
        .context("GPU work-completion callback dropped")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn render_benchmark_frame(
    widget: &mut Widget,
    renderer: &mut Renderer,
    gpu: &Gpu,
    target: &OffscreenTarget,
    input: &Input,
    dt: f32,
    index: u32,
) {
    widget.frame(dt, input);
    widget.tick(dt, input);
    let (scene, camera, hud) = widget.compose(0.0, index as f32 * dt, target.size);
    renderer.render(gpu, &target.view, target.size, scene, camera, hud);
    render_overlay(widget, gpu, target, index as f32 * dt);
}

fn render_overlay(widget: &mut Widget, gpu: &Gpu, target: &OffscreenTarget, _time: f32) {
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("headless overlay"),
        });
    widget.overlay(
        gpu,
        &mut encoder,
        &target.view,
        pocket3d::gpu::OFFSCREEN_FORMAT,
        target.size,
    );
    gpu.queue.submit([encoder.finish()]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_output_size() {
        assert_eq!(parse_size("1920x1080").unwrap(), (1920, 1080));
        assert!(parse_size("1920").is_err());
        assert!(parse_size("0x1080").is_err());
    }
}
