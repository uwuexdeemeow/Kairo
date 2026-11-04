import { beforeEach, describe, expect, it, vi } from "vitest";
import { figure, mountSpeed, type SpeedProgress } from "./speed-view";

let root: HTMLElement;
const run = vi.fn();
const text = (sel: string) => root.querySelector(sel)!.textContent;
const p = (extra: Partial<SpeedProgress>): SpeedProgress => ({ phase: "ping", pingMs: null, jitterMs: null, downMbps: null, upMbps: null, usedBytes: 0, server: null, error: null, ...extra });

beforeEach(() => {
  document.body.innerHTML = `<div id="root"></div>`;
  root = document.getElementById("root")!;
  run.mockReset();
});

describe("the speed window", () => {
  it("shows speeds to a sensible precision", () => {
    expect(figure(null)).toBe("—");
    expect(figure(48.94)).toBe("48.9");
    expect(figure(312.44)).toBe("312");
    expect(figure(0.42)).toBe("0.4");
  });

  it("fills in as the test goes, and says what it's doing", () => {
    const view = mountSpeed(root, { run });
    view.update(p({ phase: "ping" }));
    expect(text(".speed-status")).toBe("Measuring ping…");
    expect(text(".speed-down .speed-figure")).toBe("—");
    view.update(p({ phase: "download", pingMs: 12.4, jitterMs: 2.2, downMbps: 312.4, usedBytes: 36e6, server: "LHR" }));
    expect(text(".speed-status")).toBe("Testing download…");
    expect(text(".speed-down .speed-figure")).toBe("312");
    expect(text(".speed-ping")).toBe("Ping 12 ms · Jitter 2 ms");
    expect(text(".speed-server")).toBe("Cloudflare · LHR");
    expect(root.querySelector(".speed-down")!.classList.contains("live")).toBe(true);
    view.update(p({ phase: "done", pingMs: 12, jitterMs: 2, downMbps: 312.4, upMbps: 48.94, usedBytes: 81e6, server: "LHR" }));
    expect(text(".speed-status")).toBe("Done");
    expect(text(".speed-up .speed-figure")).toBe("48.9");
    expect(text(".speed-used")).toBe("Used about 81 MB");
    expect(root.querySelector(".live")).toBeNull();
  });

  it("says what went wrong", () => {
    const view = mountSpeed(root, { run });
    view.update(p({ phase: "error", error: "Couldn't reach the speed test. Check your connection." }));
    expect(text(".speed-status")).toBe("Couldn't reach the speed test. Check your connection.");
    expect(root.querySelector(".speed-status")!.classList.contains("error")).toBe(true);
  });

  it("runs again on request, clearing the last result, but not while one is running", () => {
    const view = mountSpeed(root, { run });
    const again = root.querySelector<HTMLButtonElement>(".speed-again")!;
    view.update(p({ phase: "upload", downMbps: 100 }));
    expect(again.disabled).toBe(true);
    view.update(p({ phase: "done", downMbps: 100, upMbps: 20 }));
    expect(again.disabled).toBe(false);
    again.click();
    expect(run).toHaveBeenCalledTimes(1);
    expect(text(".speed-down .speed-figure")).toBe("—");
    expect(text(".speed-status")).toBe("Measuring ping…");
  });
});
