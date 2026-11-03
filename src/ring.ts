// The app ring's window on macOS and Linux. Windows draws its own (src-tauri/src/ring/native.rs).
import "./fonts";
import "./styles.css";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { mountRing, type RingState } from "./ring-view";
import { $, followTheme } from "./ui";

followTheme();
void (async () => {
  const state = await invoke<RingState>("ring_state");
  const view = mountRing($("#ring"), state, {
    pick: (index) => void invoke("ring_pick", { index }),
    close: () => void invoke("ring_close"),
  });
  // The shortcut pressed again while the ring is open turns it.
  listen("ring:turn", () => view.turn(1));
})();
