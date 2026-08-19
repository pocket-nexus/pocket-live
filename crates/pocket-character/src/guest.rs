//! The `character` surface: the guest-facing boundary of the widget runtime.
//!
//! Mirrors the open-strike pattern — a string-keyed namespace mounted on
//! `globalThis`, facts flowing guest-ward once per tick through
//! `character.__dispatch(state, events)`, intent flowing core-ward as
//! queued [`Command`]s applied after the guest turn.

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{Result, anyhow};
use pocket_live_core::Handedness;
use pocket_mod::Guest;
use pocket_mod::qjs::{Function, Object};

#[derive(Debug, Clone)]
pub enum Command {
    SetTracking(String),
    SetExpression(String, f32),
    PlayClip { name: String, looping: bool },
    SetMaxFps(f32),
    Quit,
}

/// Per-tick facts for the guest (kept flat and cheap to build).
pub struct TickState {
    pub t: f64,
    pub blink: f32,
    pub clip: String,
    pub hovered: bool,
    pub tracking: &'static str,
    pub fps: f32,
    pub frame_ms: f32,
    pub body_tracking: &'static str,
    pub tracking_weight: f32,
    pub tracking_age_ms: f32,
    pub calibration_progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickEvent {
    Click,
    HoverStart,
    HoverEnd,
    WebShootStart(Handedness),
    WebShootEnd(Handedness),
}

impl TickEvent {
    fn name(self) -> &'static str {
        match self {
            TickEvent::Click => "click",
            TickEvent::HoverStart => "hoverStart",
            TickEvent::HoverEnd => "hoverEnd",
            TickEvent::WebShootStart(_) => "webShootStart",
            TickEvent::WebShootEnd(_) => "webShootEnd",
        }
    }

    fn hand(self) -> Option<&'static str> {
        match self {
            TickEvent::WebShootStart(Handedness::Left)
            | TickEvent::WebShootEnd(Handedness::Left) => Some("left"),
            TickEvent::WebShootStart(Handedness::Right)
            | TickEvent::WebShootEnd(Handedness::Right) => Some("right"),
            _ => None,
        }
    }
}

pub struct CharacterGuest {
    guest: Guest,
    commands: Rc<RefCell<Vec<Command>>>,
}

impl CharacterGuest {
    /// Mount the surface, install the boot table, eval the bundle.
    pub fn boot(
        bundle: &str,
        model_name: &str,
        clips: &[String],
        expressions: &[String],
    ) -> Result<CharacterGuest> {
        let guest = Guest::new()?;
        let commands: Rc<RefCell<Vec<Command>>> = Rc::default();

        let q = commands.clone();
        let model_name = model_name.to_string();
        let clips = clips.to_vec();
        let expressions = expressions.to_vec();
        guest.mount("character", move |ctx, ns| {
            let boot = Object::new(ctx.clone())?;
            boot.set("model", model_name.as_str())?;
            boot.set("clips", clips.clone())?;
            boot.set("expressions", expressions.clone())?;
            ns.set("__boot", boot)?;

            macro_rules! op {
                ($name:literal, $f:expr) => {
                    ns.set($name, Function::new(ctx.clone(), $f)?)?;
                };
            }
            let c = q.clone();
            op!("setTracking", move |mode: String| {
                c.borrow_mut().push(Command::SetTracking(mode))
            });
            let c = q.clone();
            op!("setExpression", move |name: String, w: f64| {
                c.borrow_mut().push(Command::SetExpression(name, w as f32))
            });
            let c = q.clone();
            op!("playClip", move |name: String, looping: bool| {
                c.borrow_mut().push(Command::PlayClip { name, looping })
            });
            let c = q.clone();
            op!("setMaxFps", move |fps: f64| {
                c.borrow_mut().push(Command::SetMaxFps(fps as f32))
            });
            let c = q.clone();
            op!("quit", move || c.borrow_mut().push(Command::Quit));
            Ok(())
        })?;

        guest.eval("character", bundle)?;
        Ok(CharacterGuest { guest, commands })
    }

    /// One guest turn: facts in, `frame()`, intent out.
    pub fn turn(&self, state: &TickState, events: &[TickEvent]) -> Result<Vec<Command>> {
        self.guest.with(|ctx| -> Result<()> {
            let ns: Object = ctx.globals().get("character")?;
            let Ok(dispatch) = ns.get::<_, Function>("__dispatch") else {
                return Ok(()); // policy bundle installed no callback
            };
            let s = Object::new(ctx.clone())?;
            s.set("t", state.t)?;
            s.set("blink", state.blink as f64)?;
            s.set("clip", state.clip.as_str())?;
            s.set("hovered", state.hovered)?;
            s.set("tracking", state.tracking)?;
            s.set("fps", state.fps as f64)?;
            s.set("frameMs", state.frame_ms as f64)?;
            s.set("bodyTracking", state.body_tracking)?;
            s.set("trackingWeight", state.tracking_weight as f64)?;
            s.set("trackingAgeMs", state.tracking_age_ms as f64)?;
            s.set("calibrationProgress", state.calibration_progress as f64)?;

            let evs = pocket_mod::qjs::Array::new(ctx.clone())?;
            for (i, ev) in events.iter().enumerate() {
                let o = Object::new(ctx.clone())?;
                o.set("type", ev.name())?;
                if let Some(hand) = ev.hand() {
                    o.set("hand", hand)?;
                }
                evs.set(i, o)?;
            }
            dispatch
                .call::<_, ()>((s, evs))
                .map_err(|e| anyhow!("character.__dispatch threw: {e}"))?;
            Ok(())
        })?;
        self.guest.frame(0)?;
        Ok(self.commands.borrow_mut().drain(..).collect())
    }
}
