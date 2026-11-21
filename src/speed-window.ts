import "./fonts";
import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { mountSpeed, type SpeedProgress } from "./speed-view";
import { $, followTheme } from "./ui";

// The speed test window: it opens ready, and a test starts on one click, since it spends data.

followTheme();

let running = false;
const view = mountSpeed($("#speed"), {
  run: () => {
    running = true;
    void invoke("speed_run");
  },
  stop: () => void invoke("speed_stop"),
});
listen<SpeedProgress>("speed:progress", (e) => {
  view.update(e.payload);
  if (e.payload.phase === "done" || e.payload.phase === "error" || e.payload.phase === "stopped") running = false;
});
// Asked for from the palette while open: the button is ready, nothing starts by itself.
listen("speed:again", () => {
  if (!running) view.focus();
});
