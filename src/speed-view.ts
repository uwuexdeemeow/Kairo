import { h } from "./ui";

/// The speed window's page: download and upload in large figures, ping and jitter, the server,
/// what it's doing, and Run again. The measuring is Kairo's (src-tauri/src/speedtest.rs), told
/// here step by step.

export type SpeedProgress = {
  phase: "ping" | "download" | "upload" | "done" | "error";
  pingMs: number | null;
  jitterMs: number | null;
  downMbps: number | null;
  upMbps: number | null;
  usedBytes: number;
  server: string | null;
  error: string | null;
};

export type SpeedActions = { run(): void };
export type SpeedView = { update(p: SpeedProgress): void; start(): void };

const SAYS: Record<SpeedProgress["phase"], string> = {
  ping: "Measuring ping…",
  download: "Testing download…",
  upload: "Testing upload…",
  done: "Done",
  error: "",
};

/// A speed as shown: one decimal under 100 Mbps, none above; a dash before it's known.
export function figure(mbps: number | null): string {
  if (mbps === null) return "—";
  return mbps < 100 ? mbps.toFixed(1) : String(Math.round(mbps));
}

export function mountSpeed(root: HTMLElement, act: SpeedActions): SpeedView {
  const dial = (cls: string, label: string) => {
    const fig = h("span", { class: "speed-figure" }, "—");
    return { box: h("div", { class: `speed-dial ${cls}` }, h("span", { class: "speed-label" }, label), fig, h("span", { class: "speed-unit muted" }, "Mbps")), fig };
  };
  const down = dial("speed-down", "↓ Download");
  const up = dial("speed-up", "↑ Upload");
  const ping = h("p", { class: "speed-ping" });
  const server = h("p", { class: "speed-server muted" });
  const status = h("p", { class: "speed-status", "aria-live": "polite" });
  const used = h("span", { class: "speed-used muted" });
  const again = h("button", { class: "btn speed-again" }, "Run again") as HTMLButtonElement;
  root.replaceChildren(h("div", { class: "speed" }, h("div", { class: "speed-dials" }, down.box, up.box), ping, server, status, h("div", { class: "row-buttons" }, again, used)));

  const view: SpeedView = {
    update: (p) => {
      down.fig.textContent = figure(p.downMbps);
      up.fig.textContent = figure(p.upMbps);
      down.box.classList.toggle("live", p.phase === "download");
      up.box.classList.toggle("live", p.phase === "upload");
      ping.textContent = p.pingMs === null ? "" : `Ping ${Math.round(p.pingMs)} ms${p.jitterMs === null ? "" : ` · Jitter ${Math.round(p.jitterMs)} ms`}`;
      server.textContent = p.server ? `Cloudflare · ${p.server}` : "";
      status.textContent = p.phase === "error" ? (p.error ?? "") : SAYS[p.phase];
      status.classList.toggle("error", p.phase === "error");
      used.textContent = p.usedBytes ? `Used about ${Math.round(p.usedBytes / 1e6)} MB` : "";
      again.disabled = p.phase !== "done" && p.phase !== "error";
    },
    start: () => {
      view.update({ phase: "ping", pingMs: null, jitterMs: null, downMbps: null, upMbps: null, usedBytes: 0, server: null, error: null });
      act.run();
    },
  };
  again.addEventListener("click", () => view.start());
  return view;
}
