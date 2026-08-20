import { character, onTick } from "../../../plugin-sdk/character";

console.log(`golden-horn: model=${character.__boot.model}`);
character.setTracking("none");
character.playClip("idle_loop", true);

onTick((state, events) => {
  for (const event of events) {
    if (event.type === "webShootStart") {
      // Keep the tracking gesture useful without borrowing a franchise name.
      console.log(`golden-horn: ${event.hand} thread-cast at ${state.t.toFixed(2)}s`);
    }
  }
});
