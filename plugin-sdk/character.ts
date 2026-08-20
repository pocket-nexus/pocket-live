// Stable SDK shared by character plugins. The host mounts this namespace on
// globalThis; a plugin receives state/events and queues only high-level intent.

export interface CharacterState {
  /** Sim seconds since boot. */
  t: number;
  /** Current blink expression weight 0..1. */
  blink: number;
  /** Clip currently playing. */
  clip: string;
  hovered: boolean;
  tracking: "none" | "mouse";
  /** Rendered frames per second over the last second. */
  fps: number;
  /** Mean full-frame CPU cost (ms) over the last second. */
  frameMs: number;
  /** Local body-tracking lifecycle. */
  bodyTracking: "off" | "idle" | "acquiring" | "tracking" | "holding" | "recovering";
  /** Blend weight from the animation pose to the tracked pose. */
  trackingWeight: number;
  /** Age of the newest local tracking frame, in milliseconds. */
  trackingAgeMs: number;
  /** Startup neutral-pose calibration progress, 0..1. */
  calibrationProgress: number;
}

export type CharacterEvent =
  | { type: "click" }
  | { type: "hoverStart" }
  | { type: "hoverEnd" }
  | { type: "webShootStart"; hand: "left" | "right" }
  | { type: "webShootEnd"; hand: "left" | "right" };

interface CharacterNamespace {
  __boot: {
    model: string;
    clips: string[];
    expressions: string[];
  };
  setTracking(mode: "none" | "mouse"): void;
  setExpression(name: string, weight: number): void;
  playClip(name: string, loop: boolean): void;
  setMaxFps(fps: number): void;
  quit(): void;
}

declare global {
  // eslint-disable-next-line no-var
  var character: CharacterNamespace & {
    __dispatch?: (state: CharacterState, events: CharacterEvent[]) => void;
  };
  // eslint-disable-next-line no-var
  var frame: ((buttons: number, analog: number) => void) | undefined;
}

export const character = globalThis.character;

export function onTick(
  callback: (state: CharacterState, events: CharacterEvent[]) => void,
): void {
  globalThis.character.__dispatch = callback;
}
