import { beforeEach, describe, expect, it, vi } from "vitest";

describe("whose time can be picked", () => {
  const sync = (mode: "off" | "folder" | "relay", others: number) => ({
    mode,
    folder: mode === "folder" ? "C:/Drive" : null,
    name: "Tower",
    shown: "all",
    syncedMs: null,
    error: null,
    computers: [
      { id: "a", name: "Tower", thisOne: true, updatedMs: 0 },
      ...Array.from({ length: others }, (_, i) => ({ id: `o${i}`, name: `Other ${i}`, thisOne: false, updatedMs: 0 })),
    ],
  });

  it("offers a choice whenever another computer is combined, through a folder or the relay", async () => {
    const { combinedWithOthers } = await import("./ui");
    expect(combinedWithOthers(sync("folder", 1))).toBe(true);
    expect(combinedWithOthers(sync("relay", 1))).toBe(true);
    expect(combinedWithOthers(sync("relay", 0))).toBe(false);
    expect(combinedWithOthers(sync("off", 1))).toBe(false);
  });
});

const api = vi.hoisted(() => ({ appearance: vi.fn() }));
const events = vi.hoisted(() => new Map<string, ((e: { payload: unknown }) => void)[]>());
vi.mock("./api", () => ({ api }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, fn: (e: { payload: unknown }) => void) => {
    events.set(name, [...(events.get(name) ?? []), fn]);
    return () => {};
  },
}));

import { applyTheme, esc, fmtDur, followTheme, h, pct } from "./ui";

describe("fmtDur", () => {
  it("says under a minute as <1m", () => {
    expect(fmtDur(0)).toBe("<1m");
    expect(fmtDur(29_000)).toBe("<1m");
  });

  it("gives minutes under an hour", () => {
    expect(fmtDur(60_000)).toBe("1m");
    expect(fmtDur(59 * 60_000)).toBe("59m");
  });

  it("gives hours with two-digit minutes from an hour", () => {
    expect(fmtDur(60 * 60_000)).toBe("1h 00m");
    expect(fmtDur((2 * 60 + 5) * 60_000)).toBe("2h 05m");
  });
});

describe("pct", () => {
  it("rounds to a whole percent", () => {
    expect(pct(1, 3)).toBe(33);
    expect(pct(2, 3)).toBe(67);
  });

  it("is 0 of nothing", () => {
    expect(pct(5, 0)).toBe(0);
  });
});

describe("esc", () => {
  it("escapes everything that means something in HTML", () => {
    expect(esc(`<img src=x onerror="a('b')">&`)).toBe("&#60;img src=x onerror=&#34;a(&#39;b&#39;)&#34;&#62;&#38;");
  });
});

describe("h", () => {
  it("makes an element with attributes and children", () => {
    const el = h("p", { class: "muted", "data-x": "1" }, "Hi ", h("strong", {}, "there"));
    expect(el.outerHTML).toBe('<p class="muted" data-x="1">Hi <strong>there</strong></p>');
  });
});

describe("theme", () => {
  beforeEach(() => events.clear());

  it("paints light and dark as asked", () => {
    applyTheme("light");
    expect(document.documentElement.dataset.theme).toBe("light");
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("paints the saved choice, then follows other windows' changes", async () => {
    api.appearance.mockResolvedValue("light");
    followTheme();
    await vi.waitFor(() => expect(document.documentElement.dataset.theme).toBe("light"));
    for (const fn of events.get("kairo:appearance") ?? []) fn({ payload: "dark" });
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
