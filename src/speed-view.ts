import { h } from "./ui";

/// The speed window's page: download and upload in large figures, ping and jitter, the server,
/// what it's doing, and Run again. The measuring is Kairo's (src-tauri/src/speedtest.rs), told
/// here step by step.

export type SpeedProgress = {
  phase: "ping" | "download" | "upload" | "done" | "error" | "stopped";
  pingMs: number | null;
  jitterMs: number | null;
  downMbps: number | null;
  upMbps: number | null;
  usedBytes: number;
  server: string | null;
  error: string | null;
};

/// `stop` ends a test that is running; a window without one offers none.
export type SpeedActions = { run(): void; stop?(): void };
export type SpeedView = { update(p: SpeedProgress): void; start(): void };

const SAYS: Record<SpeedProgress["phase"], string> = {
  ping: "Measuring ping…",
  download: "Testing download…",
  upload: "Testing upload…",
  done: "Done",
  error: "",
  stopped: "Stopped.",
};

/// What a finished test says: "Done: 313 Mbps down, 41.3 up".
export function doneLine(p: SpeedProgress): string {
  if (p.downMbps === null && p.upMbps === null) return "Done";
  const parts = [p.downMbps === null ? "" : `${figure(p.downMbps)} Mbps down`, p.upMbps === null ? "" : `${figure(p.upMbps)}${p.downMbps === null ? " Mbps" : ""} up`].filter(Boolean);
  return `Done: ${parts.join(", ")}`;
}

/// A speed as shown: one decimal under 100 Mbps, none above; a dash before it's known.
export function figure(mbps: number | null): string {
  if (mbps === null) return "—";
  // Rounded to a tenth first, so 99.96 reads "100", not "100.0".
  const tenth = Math.round(mbps * 10) / 10;
  return tenth < 100 ? tenth.toFixed(1) : String(Math.round(mbps));
}

export function mountSpeed(root: HTMLElement, act: SpeedActions): SpeedView {
  const dial = (cls: string, arrow: string, label: string) => {
    const fig = h("span", { class: "speed-figure" }, "—");
    return { box: h("div", { class: `speed-dial ${cls}` }, h("span", { class: "speed-label" }, h("span", { "aria-hidden": "true" }, `${arrow} `), label), fig, h("span", { class: "speed-unit muted" }, "Mbps")), fig };
  };
  const down = dial("speed-down", "↓", "Download");
  const up = dial("speed-up", "↑", "Upload");
  const ping = h("p", { class: "speed-ping" });
  const server = h("p", { class: "speed-server muted" });
  const status = h("p", { class: "speed-status", "aria-live": "polite" });
  const used = h("span", { class: "speed-used muted" });
  const again = h("button", { class: "btn speed-again" }, "Run again") as HTMLButtonElement;
  // Ends a test that is spending data, rather than waiting for it or closing the window.
  const stopButton = h("button", { class: "link-btn speed-stop", hidden: "" }, "Stop") as HTMLButtonElement;
  stopButton.addEventListener("click", () => act.stop?.());
  root.replaceChildren(h("div", { class: "speed" }, h("h1", { class: "sr-only" }, "Speed test"), h("div", { class: "speed-dials" }, down.box, up.box), ping, server, status, h("div", { class: "row-buttons" }, again, stopButton, used)));

  /// The last figures heard, which a failure keeps: its event carries nothing measured.
  let last: SpeedProgress | null = null;
  const view: SpeedView = {
    update: (heard) => {
      // A failure falls back on what was measured before it, so a lost upload doesn't wipe the download.
      const p: SpeedProgress =
        (heard.phase === "error" || heard.phase === "stopped") && last
          ? { ...heard, pingMs: heard.pingMs ?? last.pingMs, jitterMs: heard.jitterMs ?? last.jitterMs, downMbps: heard.downMbps ?? last.downMbps, upMbps: heard.upMbps ?? last.upMbps, usedBytes: heard.usedBytes || last.usedBytes, server: heard.server ?? last.server }
          : heard;
      last = p.phase === "ping" && p.downMbps === null && p.pingMs === null ? null : p;
      down.fig.textContent = figure(p.downMbps);
      up.fig.textContent = figure(p.upMbps);
      down.box.classList.toggle("live", p.phase === "download");
      up.box.classList.toggle("live", p.phase === "upload");
      ping.textContent = p.pingMs === null ? "" : `Ping ${Math.round(p.pingMs)} ms${p.jitterMs === null ? "" : ` · Jitter ${Math.round(p.jitterMs)} ms`}`;
      server.textContent = p.server ? `Cloudflare · ${p.server}` : "";
      // Done says the result, so a screen reader hears it, not only that it finished.
      status.textContent = p.phase === "error" ? (p.error ?? "") : p.phase === "done" ? doneLine(p) : SAYS[p.phase];
      status.classList.toggle("error", p.phase === "error");
      used.textContent = p.usedBytes ? `Used about ${Math.round(p.usedBytes / 1e6)} MB` : "";
      // Held back while a test runs, but not disabled: a disabled button would drop the keyboard.
      const testing = p.phase !== "done" && p.phase !== "error" && p.phase !== "stopped";
      stopButton.hidden = !testing;
      again.setAttribute("aria-disabled", String(testing));
      again.textContent = testing ? "Testing…" : p.phase === "error" ? "Try again" : "Run again";
    },
    start: () => {
      view.update({ phase: "ping", pingMs: null, jitterMs: null, downMbps: null, upMbps: null, usedBytes: 0, server: null, error: null });
      act.run();
    },
  };
  again.addEventListener("click", () => {
    if (again.getAttribute("aria-disabled") !== "true") view.start();
  });
  // The window's one control has the keyboard from the start, so Enter runs it again when done.
  again.focus();
  return view;
}
