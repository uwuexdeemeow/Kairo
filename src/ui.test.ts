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

describe("saying why something failed", () => {
  it("shows a reason that reads like one, and a calm line for raw exceptions", async () => {
    const { why } = await import("./ui");
    expect(why("Strict mode is on: end the current session first.")).toBe("Strict mode is on: end the current session first.");
    expect(why(new Error("Error: no such folder"))).toBe("no such folder");
    for (const raw of [new TypeError("Cannot read properties of undefined (reading 'invoke')"), new Error("Cannot read properties of null (reading 'filter')"), "database is locked", "", undefined, "[object Object]"]) {
      expect(why(raw)).toBe("Try again in a moment.");
    }
  });
});

describe("arrow keys across a row of chart marks", () => {
  const key = (el: Element, key: string) => el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
  const marks = (n: number) => {
    document.body.replaceChildren(...Array.from({ length: n }, () => document.createElement("span")));
    return [...document.body.children] as HTMLElement[];
  };

  it("is one Tab stop, with the arrows, Home and End moving between marks", async () => {
    const { rove } = await import("./ui");
    const m = marks(4);
    rove(m);
    expect(m.map((el) => el.getAttribute("tabindex"))).toEqual(["0", "-1", "-1", "-1"]);
    m[0].focus();
    key(m[0], "ArrowRight");
    expect(document.activeElement).toBe(m[1]);
    expect(m.map((el) => el.getAttribute("tabindex"))).toEqual(["-1", "0", "-1", "-1"]);
    key(m[1], "End");
    expect(document.activeElement).toBe(m[3]);
    key(m[3], "ArrowRight");
    expect(document.activeElement).toBe(m[3]);
    key(m[3], "Home");
    expect(document.activeElement).toBe(m[0]);
  });

  it("moves by a row with up and down when it has columns, and skips disabled marks", async () => {
    const { rove } = await import("./ui");
    const m = marks(6);
    rove(m, 3);
    m[1].focus();
    key(m[1], "ArrowDown");
    expect(document.activeElement).toBe(m[4]);
    key(m[4], "ArrowUp");
    expect(document.activeElement).toBe(m[1]);

    document.body.replaceChildren(...Array.from({ length: 3 }, () => document.createElement("button")));
    const b = [...document.body.children] as HTMLButtonElement[];
    b[1].disabled = true;
    rove(b);
    b[0].focus();
    key(b[0], "ArrowRight");
    expect(document.activeElement).toBe(b[2]);
  });

  it("keeps the arrows from reaching the page's own shortcuts", async () => {
    const { rove } = await import("./ui");
    const m = marks(2);
    rove(m);
    const page = vi.fn();
    document.addEventListener("keydown", page, { once: true });
    key(m[0], "ArrowRight");
    expect(page).not.toHaveBeenCalled();
  });
});
