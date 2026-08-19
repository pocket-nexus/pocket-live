// The upstream submodule is intentionally pinned. Pocket Live needs two small
// window-loop capabilities that have not landed at that commit: physical-pixel
// sizing for 1080p output on Retina and deadline-based frame pacing. Apply the
// source patch deterministically at setup/build time instead of committing a
// dirty submodule pointer.
import { join } from "node:path";

const file = join(
  import.meta.dir,
  "../vendor/pocketjs/pocket3d/crates/pocket3d/src/app.rs",
);
let source = await Bun.file(file).text();

function replaceOnce(before: string, after: string): void {
  if (!source.includes(before)) {
    throw new Error(`PocketJS patch anchor is missing:\n${before.slice(0, 160)}`);
  }
  source = source.replace(before, after);
}

let changed = false;
if (!source.includes("pub fn run_physical")) {
  replaceOnce(
    `pub fn run(config: AppConfig, game: impl Game) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = WinitApp {
        config,
        game,
        state: None,
        error: None,
    };`,
    `pub fn run(config: AppConfig, game: impl Game) -> Result<()> {
    run_with_size_mode(config, game, false)
}

/// Run with AppConfig::size interpreted as physical pixels. Video-output
/// applications need this so 1920x1080 stays 1920x1080 on a Retina display.
pub fn run_physical(config: AppConfig, game: impl Game) -> Result<()> {
    run_with_size_mode(config, game, true)
}

fn run_with_size_mode(config: AppConfig, game: impl Game, physical_size: bool) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = WinitApp {
        config,
        game,
        physical_size,
        state: None,
        error: None,
    };`,
  );
  replaceOnce(
    `struct WinitApp<G: Game> {
    config: AppConfig,
    game: G,
    state: Option<WindowState>,`,
    `struct WinitApp<G: Game> {
    config: AppConfig,
    game: G,
    physical_size: bool,
    state: Option<WindowState>,`,
  );
  replaceOnce(
    `        let attrs = Window::default_attributes()
            .with_title(self.config.title.clone())
            .with_inner_size(winit::dpi::LogicalSize::new(
                self.config.size.0,
                self.config.size.1,
            ))
            .with_transparent(self.config.transparent)`,
    `        let mut attrs = Window::default_attributes()
            .with_title(self.config.title.clone())
            .with_transparent(self.config.transparent)`,
  );
  replaceOnce(
    `            } else {
                WindowLevel::Normal
            });
        let window = Arc::new(event_loop.create_window(attrs)?);`,
    `            } else {
                WindowLevel::Normal
            });
        attrs = if self.physical_size {
            attrs.with_inner_size(winit::dpi::PhysicalSize::new(
                self.config.size.0,
                self.config.size.1,
            ))
        } else {
            attrs.with_inner_size(winit::dpi::LogicalSize::new(
                self.config.size.0,
                self.config.size.1,
            ))
        };
        let window = Arc::new(event_loop.create_window(attrs)?);`,
  );
  changed = true;
}

if (!source.includes("next_redraw: Instant")) {
  replaceOnce(
    `    start: Instant,
    last_frame: Instant,
    mouse_captured: bool,`,
    `    start: Instant,
    last_frame: Instant,
    next_redraw: Instant,
    mouse_captured: bool,`,
  );
  replaceOnce(
    `        let mut state = WindowState {
            window,`,
    `        let now = Instant::now();
        let mut state = WindowState {
            window,`,
  );
  replaceOnce(
    `            start: Instant::now(),
            last_frame: Instant::now(),
            mouse_captured: false,`,
    `            start: now,
            last_frame: now,
            next_redraw: now,
            mouse_captured: false,`,
  );
  replaceOnce(
    `    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &self.state else { return };
        let Some(max_fps) = self.config.max_fps else {
            state.window.request_redraw();
            return;
        };
        // Frame-paced mode: sleep until the next frame is due instead of
        // redrawing every time the loop wakes.
        let interval = Duration::from_secs_f32(1.0 / max_fps.max(1.0));
        let due = state.last_frame + interval;
        if Instant::now() >= due {
            state.window.request_redraw();
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(due));
        }
    }`,
    `    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        let Some(max_fps) = self.config.max_fps else {
            state.window.request_redraw();
            return;
        };
        // Advance a stable deadline instead of deriving the next frame from
        // the last presentation time, which accumulates work-time drift.
        let interval = Duration::from_secs_f32(1.0 / max_fps.max(1.0));
        let now = Instant::now();
        if now >= state.next_redraw {
            state.window.request_redraw();
            let next = state.next_redraw + interval;
            state.next_redraw = if next <= now { now + interval } else { next };
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(state.next_redraw));
        }
    }`,
  );
  changed = true;
}

if (changed) {
  await Bun.write(file, source);
  console.log("applied Pocket Live window patch to pinned PocketJS");
} else {
  console.log("Pocket Live window patch already applied");
}
