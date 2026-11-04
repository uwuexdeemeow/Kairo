import "./fonts";
import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { mountSpeed, type SpeedProgress } from "./speed-view";
import { $, followTheme } from "./ui";

// The speed test window: a test starts as it opens.

followTheme();

let running = false;
const view = mountSpeed($("#speed"), {
  run: () => {
    running = true;
    void invoke("speed_run");
  },
});
listen<SpeedProgress>("speed:progress", (e) => {
  view.update(e.payload);
  if (e.payload.phase === "done" || e.payload.phase === "error") running = false;
});
// Asked for again from the palette while open: another test, unless one is running.
listen("speed:again", () => {
  if (!running) view.start();
});
view.start();
