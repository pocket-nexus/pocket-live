// Default character policy. The native core owns continuous simulation;
// plugins decide which clip and reactions make this particular character.
import { character, onTick } from "../../../plugin-sdk/character";

console.log(
  `pocket-character: model=${character.__boot.model}`,
  `clips=[${character.__boot.clips.join(", ")}]`,
  `expressions=${character.__boot.expressions.length}`,
);

character.setTracking("none");
character.playClip("idle_loop", true);

let lastStatsLog = 0;
onTick((state, events) => {
  for (const event of events) {
    if (event.type === "click") {
      console.log("character: click at t =", state.t.toFixed(2));
    }
    if (event.type === "webShootStart") {
      console.log(`character: ${event.hand} web shoot at t=${state.t.toFixed(2)}`);
    }
  }
  if (state.t - lastStatsLog >= 60) {
    lastStatsLog = state.t;
    console.log(
      `character: t=${state.t.toFixed(0)}s fps=${state.fps.toFixed(1)} frameMs=${state.frameMs.toFixed(2)}`,
    );
  }
});
