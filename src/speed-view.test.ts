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
  it("opens ready, says what a test costs, and starts nothing by itself", () => {
    const view = mountSpeed(root, { run });
    expect(run).not.toHaveBeenCalled();
    expect(text(".speed-status")).toBe("Uses up to 100 MB of data.");
    const button = root.querySelector<HTMLButtonElement>(".speed-again")!;
    expect(button.textContent).toBe("Start test");
    expect(document.activeElement).toBe(button);
    button.click();
    expect(run).toHaveBeenCalledTimes(1);
    expect(text(".speed-status")).toBe("Measuring ping…");
    view.idle();
    expect(text(".speed-status")).toBe("Uses up to 100 MB of data.");
  });

  it("gives the keyboard to the button when Stop goes away", () => {
    const view = mountSpeed(root, { run, stop: vi.fn() });
    view.update(p({ phase: "download" }));
    const stop = root.querySelector<HTMLButtonElement>(".speed-stop")!;
    stop.focus();
    view.update(p({ phase: "stopped" }));
    expect(document.activeElement).toBe(root.querySelector(".speed-again"));
  });

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
    expect(text(".speed-status")).toBe("Done: 312 Mbps down, 48.9 up");
    expect(text(".speed-up .speed-figure")).toBe("48.9");
    expect(text(".speed-used")).toBe("Used about 81 MB");
    expect(root.querySelector(".live")).toBeNull();
  });

  it("says what went wrong", () => {
    const view = mountSpeed(root, { run });
    view.update(p({ phase: "error", error: "Couldn't reach the speed test. Check your connection." }));
    expect(text(".speed-status")).toBe("Couldn't reach the speed test. Check your connection.");
    expect(root.querySelector(".speed-status")!.classList.contains("error")).toBe(true);
    // The way back says it is a second go.
    expect(text(".speed-again")).toBe("Try again");
  });

  it("keeps what was measured when a later step fails, and what it cost", () => {
    const view = mountSpeed(root, { run });
    view.update(p({ phase: "upload", pingMs: 12.4, jitterMs: 2.2, downMbps: 312.4, usedBytes: 40e6, server: "LHR" }));
    // The error event carries nothing measured: the figures already on the page stay.
    view.update(p({ phase: "error", error: "Couldn't reach the speed test. Check your connection." }));
    expect(text(".speed-down .speed-figure")).toBe("312");
    expect(text(".speed-ping")).toBe("Ping 12 ms · Jitter 2 ms");
    expect(text(".speed-server")).toBe("Cloudflare · LHR");
    expect(text(".speed-used")).toBe("Used about 40 MB");
    expect(text(".speed-up .speed-figure")).toBe("—");
    // A new test starts from nothing, not from the failed one.
    root.querySelector<HTMLButtonElement>(".speed-again")!.click();
    expect(text(".speed-down .speed-figure")).toBe("—");
    expect(text(".speed-used")).toBe("");
  });

  it("offers Stop while a test runs, and keeps what it had when stopped", () => {
    const stop = vi.fn();
    const view = mountSpeed(root, { run, stop });
    const button = () => root.querySelector<HTMLButtonElement>(".speed-stop")!;
    // Not while idle or done.
    expect(button().hidden).toBe(true);
    view.update(p({ phase: "download", pingMs: 12.4, downMbps: 250 }));
    expect(button().hidden).toBe(false);
    button().click();
    expect(stop).toHaveBeenCalledTimes(1);
    // What the backend then says: stopped, with nothing measured in the event itself.
    view.update(p({ phase: "stopped" }));
    expect(text(".speed-status")).toBe("Stopped.");
    expect(text(".speed-down .speed-figure")).toBe("250");
    expect(text(".speed-ping")).toBe("Ping 12 ms");
    expect(button().hidden).toBe(true);
    expect(text(".speed-again")).toBe("Run again");
    expect(root.querySelector(".speed-again")!.getAttribute("aria-disabled")).toBe("false");
  });

  it("never rounds a speed up into a precision it doesn't have", () => {
    expect(figure(99.96)).toBe("100");
    expect(figure(99.94)).toBe("99.9");
  });

  it("runs again on request, clearing the last result, but not while one is running", () => {
    const view = mountSpeed(root, { run });
    const again = root.querySelector<HTMLButtonElement>(".speed-again")!;
    view.update(p({ phase: "upload", downMbps: 100 }));
    // Held back but not disabled, so the keyboard stays on it.
    expect(again.getAttribute("aria-disabled")).toBe("true");
    expect(again.textContent).toBe("Testing…");
    again.click();
    expect(run).not.toHaveBeenCalled();
    view.update(p({ phase: "done", downMbps: 100, upMbps: 20 }));
    expect(again.getAttribute("aria-disabled")).toBe("false");
    expect(again.textContent).toBe("Run again");
    expect(document.activeElement).toBe(again);
    again.click();
    expect(run).toHaveBeenCalledTimes(1);
    expect(text(".speed-down .speed-figure")).toBe("—");
    expect(text(".speed-status")).toBe("Measuring ping…");
  });
});
