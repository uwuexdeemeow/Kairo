import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PaletteItem, PaletteResults } from "./api";
import { paletteBody as markup } from "./test-palette";

const api = vi.hoisted(() => ({
  paletteQuery: vi.fn(),
  paletteRun: vi.fn(),
  paletteRunSecond: vi.fn(),
  paletteOpen: vi.fn(),
  paletteIcons: vi.fn(),
  paletteCard: vi.fn(),
  paletteHide: vi.fn(),
  paletteGlass: vi.fn(),
}));
const events = vi.hoisted(() => new Map<string, ((e: { payload: unknown }) => void)[]>());
vi.mock("./api", () => ({ api }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, fn: (e: { payload: unknown }) => void) => {
    events.set(name, [...(events.get(name) ?? []), fn]);
    return () => {};
  },
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ setSize: async () => {} }) }));
vi.mock("@tauri-apps/api/dpi", () => ({ LogicalSize: class {} }));

const $ = <T extends HTMLElement = HTMLElement>(sel: string) => document.querySelector<T>(sel)!;
const emit = (name: string, payload: unknown) => events.get(name)?.forEach((fn) => fn({ payload }));
const key = (k: string, init: KeyboardEventInit = {}) => document.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, ...init }));
const titles = () => [...document.querySelectorAll(".palette-title")].map((t) => t.textContent);
const labels = () => [...document.querySelectorAll(".palette-label")].map((t) => t.textContent);
const hint = () => $("#palette-second").textContent?.replace(/\s+/g, " ").trim();

const item = (title: string, extra: Partial<PaletteItem> = {}): PaletteItem => ({
  title,
  at: [],
  sub: null,
  detail: null,
  icon: { kind: "glyph", glyph: "day", tone: "" },
  section: "Actions",
  second: null,
  preview: false,
  ...extra,
});
const file = (title: string, fileKind: "folder" | "image" | "document" | "other" = "document"): PaletteItem =>
  item(title, { section: "Files", second: "Show in folder", icon: { kind: "file", path: `/home/a/${title}`, fileKind } });
const results = (seq: number, rows: PaletteItem[]): PaletteResults => ({ seq, rows, tiles: [], chips: [] });

/// Opens the palette as its window does, on a computer whose user agent contains `ua`.
async function open(first: PaletteResults, ua = "Linux") {
  Object.defineProperty(navigator, "userAgent", { value: `Mozilla/5.0 (${ua})`, configurable: true });
  api.paletteQuery.mockResolvedValue(first);
  await import("./palette");
  await vi.waitFor(() => expect(titles().length).toBe(first.rows.length));
}

/// Each import of the page hooks the document again; the last test's hooks must not hear this one's keys.
const hooked: [string, EventListener][] = [];
const addListener = document.addEventListener.bind(document);
document.addEventListener = ((type: string, fn: EventListener, opts?: boolean) => {
  hooked.push([type, fn]);
  addListener(type, fn, opts);
}) as typeof document.addEventListener;

beforeEach(() => {
  for (const [type, fn] of hooked.splice(0)) document.removeEventListener(type, fn);
  vi.resetModules();
  events.clear();
  for (const fn of Object.values(api)) fn.mockReset();
  api.paletteOpen.mockResolvedValue({ appearance: "dark", footer: "", compact: false });
  api.paletteIcons.mockResolvedValue({});
  api.paletteCard.mockResolvedValue(null);
  api.paletteRun.mockResolvedValue(null);
  api.paletteGlass.mockResolvedValue(undefined);
  document.body.innerHTML = markup;
});

describe("the compact look", () => {
  it("opens as the search bar alone, and drops the results down once something's typed", async () => {
    api.paletteOpen.mockResolvedValue({ appearance: "dark", footer: "", compact: true });
    await open(results(1, []));
    await vi.waitFor(() => expect($("#palette").classList.contains("bare")).toBe(true));
    $<HTMLInputElement>("#palette-input").value = "chrono";
    api.paletteQuery.mockResolvedValueOnce(results(2, [item("Open Chrono")]));
    $<HTMLInputElement>("#palette-input").dispatchEvent(new Event("input"));
    await vi.waitFor(() => expect(titles()).toEqual(["Open Chrono"]));
    expect($("#palette").classList.contains("bare")).toBe(false);
    $<HTMLInputElement>("#palette-input").value = "";
    api.paletteQuery.mockResolvedValueOnce(results(3, []));
    $<HTMLInputElement>("#palette-input").dispatchEvent(new Event("input"));
    await vi.waitFor(() => expect($("#palette").classList.contains("bare")).toBe(true));
  });

  it("isn't how the full look opens", async () => {
    await open(results(1, [item("Open Chrono")]));
    expect($("#palette").classList.contains("bare")).toBe(false);
  });
});

describe("the glass", () => {
  it("wears the chosen ground", async () => {
    api.paletteOpen.mockResolvedValue({ appearance: "walnut", footer: "", compact: false });
    await open(results(1, [item("Open Chrono")]));
    await vi.waitFor(() => expect(document.documentElement.dataset).toMatchObject({ theme: "dark", ground: "walnut" }));
  });

  it("is frosted on a Mac, placed under the panel each time the window fits it", async () => {
    await open(results(1, [item("Open Chrono")]), "Macintosh");
    expect(document.documentElement.dataset.glass).toBe("frosted");
    expect(api.paletteGlass).toHaveBeenCalledWith(expect.objectContaining({ left: expect.any(Number), top: expect.any(Number), right: expect.any(Number), bottom: expect.any(Number) }));
  });

  it("is only a tint on Linux, with nothing for macOS to place", async () => {
    await open(results(1, [item("Open Chrono")]));
    expect(document.documentElement.dataset.glass).toBe("tinted");
    expect(api.paletteGlass).not.toHaveBeenCalled();
  });
});

describe("files", () => {
  it("appear under a Files label after the other rows, for the current query only", async () => {
    await open(results(3, [item("Open Chrono")]));
    emit("palette:files", { seq: 2, rows: [file("old.txt")] });
    expect(titles()).toEqual(["Open Chrono"]);
    emit("palette:files", { seq: 3, rows: [file("notes.txt")] });
    expect(titles()).toEqual(["Open Chrono", "notes.txt"]);
    expect(labels()).toEqual(["Actions", "Files"]);
  });

  it("don't move the selection, and Enter still runs the same item", async () => {
    await open(results(3, [item("Open Chrono"), item("Settings")]));
    key("ArrowDown");
    emit("palette:files", { seq: 3, rows: [file("a.txt"), file("b.txt")] });
    expect($(".selected .palette-title").textContent).toBe("Settings");
    key("Enter");
    expect(api.paletteRun).toHaveBeenCalledWith(3, 1);
  });

  it("draw a glyph by kind on Linux", async () => {
    await open(results(1, [file("pics", "folder")]));
    expect(document.querySelector(".palette-glyph")).not.toBeNull();
    expect(api.paletteIcons).not.toHaveBeenCalled();
  });

  it("ask for the file's icon elsewhere", async () => {
    await open(results(1, [file("pics", "folder")]), "Macintosh");
    expect(api.paletteIcons).toHaveBeenCalledWith(["/home/a/pics"]);
  });
});

describe("second actions", () => {
  it("run on Ctrl+Enter and Meta+Enter", async () => {
    await open(results(4, [file("a.txt"), file("b.txt")]));
    key("ArrowDown");
    key("Enter", { ctrlKey: true });
    expect(api.paletteRunSecond).toHaveBeenCalledWith(4, 1);
    key("Enter", { metaKey: true });
    expect(api.paletteRunSecond).toHaveBeenCalledTimes(2);
    expect(api.paletteRun).not.toHaveBeenCalled();
  });

  it("show their label in the footer, with the key for the computer", async () => {
    const setting = item("Dark mode", { second: "Open in settings" });
    await open(results(1, [file("a.txt"), setting, item("Plain")]));
    expect(hint()).toBe("Ctrl Enter Show in folder");
    key("ArrowDown");
    expect(hint()).toBe("Ctrl Enter Open in settings");
    key("ArrowDown");
    expect(hint()).toBe("");
  });

  it("use Command on a Mac", async () => {
    await open(results(1, [file("a.txt")]), "Macintosh");
    expect(hint()).toBe("⌘ Enter Show in folder");
  });
});

describe("switches", () => {
  it("show the fresh results after Enter, keep the selection and stay open", async () => {
    const on = item("Pause tracking", { detail: "On" });
    await open(results(1, [item("Open Chrono"), on]));
    key("ArrowDown");
    api.paletteRun.mockResolvedValueOnce(results(2, [item("Open Chrono"), item("Pause tracking", { detail: "Off" })]));
    key("Enter");
    await vi.waitFor(() => expect($(".selected .palette-detail").textContent).toBe("Off"));
    expect($(".selected .palette-title").textContent).toBe("Pause tracking");
    expect(api.paletteHide).not.toHaveBeenCalled();
    api.paletteRun.mockResolvedValueOnce(null);
    key("Enter");
    expect(api.paletteRun).toHaveBeenLastCalledWith(2, 1);
  });
});

describe("refused switches", () => {
  it("say why on their row's second line, keeping their state and the selection", async () => {
    const why = "Strict mode is on, so these stay as they are until the focus session ends.";
    await open(results(1, [item("Open Chrono"), item("Focus blocking", { detail: "On", sub: "Chrono settings" })]));
    key("ArrowDown");
    api.paletteRun.mockResolvedValueOnce(results(2, [item("Open Chrono"), item("Focus blocking", { detail: "On", sub: why })]));
    key("Enter");
    await vi.waitFor(() => expect($(".selected .palette-sub").textContent).toBe(why));
    expect($(".selected .palette-title").textContent).toBe("Focus blocking");
    expect($(".selected .palette-detail").textContent).toBe("On");
    expect(document.querySelector(".palette-error")).toBeNull();
  });
});

describe("headings", () => {
  it("never call a file the top hit", async () => {
    Object.defineProperty(navigator, "userAgent", { value: "Mozilla/5.0 (Linux)", configurable: true });
    api.paletteQuery.mockResolvedValue(results(1, []));
    await import("./palette");
    $<HTMLInputElement>("#palette-input").value = "budget";
    api.paletteQuery.mockResolvedValueOnce(results(2, [file("budget.txt")]));
    $<HTMLInputElement>("#palette-input").dispatchEvent(new Event("input"));
    await vi.waitFor(() => expect(titles()).toEqual(["budget.txt"]));
    expect(labels()).toEqual(["Files"]);
  });

  it("still lead with the top hit for anything else", async () => {
    Object.defineProperty(navigator, "userAgent", { value: "Mozilla/5.0 (Linux)", configurable: true });
    api.paletteQuery.mockResolvedValue(results(1, []));
    await import("./palette");
    $<HTMLInputElement>("#palette-input").value = "chrono";
    api.paletteQuery.mockResolvedValueOnce(results(2, [item("Open Chrono"), file("chrono.txt")]));
    $<HTMLInputElement>("#palette-input").dispatchEvent(new Event("input"));
    await vi.waitFor(() => expect(titles()).toEqual(["Open Chrono", "chrono.txt"]));
    expect(labels()).toEqual(["Top hit", "Files"]);
  });

  it("keep the calculator's heading and draw its glyph", async () => {
    Object.defineProperty(navigator, "userAgent", { value: "Mozilla/5.0 (Linux)", configurable: true });
    api.paletteQuery.mockResolvedValue(results(1, []));
    await import("./palette");
    $<HTMLInputElement>("#palette-input").value = "5 km in mi";
    const sum = item("3.10686 mi", { section: "Calculator", second: "Copy number", icon: { kind: "glyph", glyph: "calc", tone: "accent" } });
    api.paletteQuery.mockResolvedValueOnce(results(2, [sum, item("Open Chrono")]));
    $<HTMLInputElement>("#palette-input").dispatchEvent(new Event("input"));
    await vi.waitFor(() => expect(titles()).toEqual(["3.10686 mi", "Open Chrono"]));
    expect(labels()).toEqual(["Calculator", "Actions"]);
    expect(document.querySelector(".palette-glyph svg path")).not.toBeNull();
  });
});

describe("racing answers", () => {
  const flip = () => results(2, [item("Pause tracking", { detail: "Off" })]);
  const deferred = <T>() => {
    let resolve!: (v: T) => void;
    const promise = new Promise<T>((r) => (resolve = r));
    return { promise, resolve };
  };
  const type = () => $<HTMLInputElement>("#palette-input").dispatchEvent(new Event("input"));
  const settle = () => new Promise((r) => setTimeout(r));

  it("draw a query that lands after an Enter on the rows it replaced, so the next Enter works", async () => {
    await open(results(1, [item("Old row")]));
    const b = deferred<PaletteResults>();
    api.paletteQuery.mockReturnValueOnce(b.promise);
    type();
    // The query was asked before Enter, so Kairo has moved on and refuses the old row.
    expect(api.paletteQuery).toHaveBeenCalledTimes(2);
    api.paletteRun.mockRejectedValueOnce("The results changed as you chose; try again.");
    key("Enter");
    expect(api.paletteRun).toHaveBeenCalledWith(1, 0);
    expect(api.paletteRun.mock.invocationCallOrder[0]).toBeGreaterThan(api.paletteQuery.mock.invocationCallOrder[1]);
    await vi.waitFor(() => expect($(".palette-error").textContent).toBe("The results changed as you chose; try again."));
    b.resolve(results(2, [item("New row")]));
    await vi.waitFor(() => expect(titles()).toEqual(["New row"]));
    expect(document.querySelector(".palette-error")).toBeNull();
    key("Enter");
    expect(api.paletteRun).toHaveBeenLastCalledWith(2, 0);
  });

  it("ignore an older query that resolves after a flip", async () => {
    await open(results(1, [item("Pause tracking", { detail: "On" })]));
    const late = deferred<PaletteResults>();
    api.paletteQuery.mockReturnValueOnce(late.promise);
    type();
    api.paletteRun.mockResolvedValueOnce(flip());
    key("Enter");
    // The query went out first, and the flip's answer came back before it.
    expect(api.paletteRun.mock.invocationCallOrder[0]).toBeGreaterThan(api.paletteQuery.mock.invocationCallOrder[1]);
    await vi.waitFor(() => expect($(".palette-detail").textContent).toBe("Off"));
    late.resolve(results(0, [item("Stale"), item("Rows")]));
    await settle();
    expect(titles()).toEqual(["Pause tracking"]);
  });

  it("ignore a flip's answer once the user has typed again", async () => {
    await open(results(1, [item("Pause tracking", { detail: "On" })]));
    const slow = deferred<PaletteResults | null>();
    api.paletteRun.mockReturnValueOnce(slow.promise);
    key("Enter");
    api.paletteQuery.mockResolvedValueOnce(results(3, [item("Newer")]));
    type();
    // The flip started before the typing.
    expect(api.paletteQuery.mock.invocationCallOrder[1]).toBeGreaterThan(api.paletteRun.mock.invocationCallOrder[0]);
    await vi.waitFor(() => expect(titles()).toEqual(["Newer"]));
    slow.resolve(flip());
    await settle();
    expect(titles()).toEqual(["Newer"]);
  });
});

describe("running", () => {
  it("does nothing on Ctrl+Enter for a row without a second action", async () => {
    await open(results(1, [item("Plain")]));
    key("Enter", { ctrlKey: true });
    expect(api.paletteRunSecond).not.toHaveBeenCalled();
    expect(document.querySelector(".palette-error")).toBeNull();
  });

  it("shows an error from either run", async () => {
    await open(results(1, [file("a.txt")]));
    api.paletteRun.mockRejectedValueOnce("no luck");
    key("Enter");
    await vi.waitFor(() => expect($(".palette-error").textContent).toBe("no luck"));
    $(".palette-error").remove();
    api.paletteRunSecond.mockRejectedValueOnce("not there");
    key("Enter", { ctrlKey: true });
    await vi.waitFor(() => expect($(".palette-error").textContent).toBe("not there"));
  });
});

describe("modes", () => {
  const github = { key: "web:gh", name: "GitHub", glyph: "search", hint: "Search GitHub…" };
  const field = () => $<HTMLInputElement>("#palette-input");
  const typeText = (text: string) => {
    field().value = text;
    field().dispatchEvent(new Event("input"));
  };
  const bubbleText = () => ($("#palette-bubble").hidden ? null : $("#palette-bubble").textContent);

  it("a mode row puts the palette in that mode, with the bubble and an empty input", async () => {
    await open(results(1, [item("GitHub Desktop", { section: "Apps" }), item("Search GitHub", { section: "Modes" })]));
    field().value = "github";
    api.paletteRun.mockResolvedValueOnce({ ...results(2, [item("Type what to search GitHub for", { section: "Web" })]), mode: github });
    key("ArrowDown");
    key("Enter");
    await vi.waitFor(() => expect(bubbleText()).toBe("GitHub"));
    expect(field().value).toBe("");
    expect(field().placeholder).toBe("Search GitHub…");
    // What's typed next goes to GitHub.
    api.paletteQuery.mockResolvedValueOnce({ ...results(3, [item("Search GitHub for “tauri”", { section: "Web" })]), mode: github });
    typeText("tauri");
    expect(api.paletteQuery).toHaveBeenLastCalledWith("tauri", "web:gh");
  });

  it("Tab takes a mode row too", async () => {
    await open(results(1, [item("Calculator", { section: "Modes" })]));
    key("Tab");
    expect(api.paletteRun).toHaveBeenCalledWith(1, 0);
  });

  it("a short word and a space becomes the bubble, keeping what came after it", async () => {
    await open(results(1, [item("Open Chrono")]));
    api.paletteQuery.mockResolvedValueOnce({ ...results(2, [item("Search GitHub for “k”", { section: "Web" })]), mode: github, text: "k" });
    typeText("gh k");
    await vi.waitFor(() => expect(bubbleText()).toBe("GitHub"));
    expect(field().value).toBe("k");
    expect(labels()).toEqual(["Web"]);
  });

  it("Backspace on an empty input, or Esc, leaves the mode; a second Esc closes", async () => {
    await open(results(1, [item("Open Chrono")]));
    api.paletteQuery.mockResolvedValueOnce({ ...results(2, []), mode: github, text: "" });
    typeText("gh ");
    await vi.waitFor(() => expect(bubbleText()).toBe("GitHub"));
    api.paletteQuery.mockResolvedValue(results(3, [item("Open Chrono")]));
    field().setSelectionRange(0, 0);
    key("Backspace");
    expect(bubbleText()).toBeNull();
    expect(field().placeholder).toBe("Open an app, jump to a day, or type a command");
    expect(api.paletteQuery).toHaveBeenLastCalledWith("", null);

    api.paletteQuery.mockResolvedValueOnce({ ...results(4, []), mode: github, text: "" });
    typeText("gh ");
    await vi.waitFor(() => expect(bubbleText()).toBe("GitHub"));
    key("Escape");
    expect(bubbleText()).toBeNull();
    expect(api.paletteHide).not.toHaveBeenCalled();
    key("Escape");
    expect(api.paletteHide).toHaveBeenCalled();
  });
});

describe("files mode", () => {
  it("says it's looking, then shows the files in that row's place", async () => {
    const files = { key: "files", name: "Files", glyph: "search", hint: "Type part of a file's name…" };
    await open({ ...results(5, [item("Looking for files…", { section: "Files" })]), mode: files });
    expect(titles()).toEqual(["Looking for files…"]);
    emit("palette:files", { seq: 5, rows: [file("budget.xlsx")], replace: true });
    expect(titles()).toEqual(["budget.xlsx"]);
    expect(document.querySelector(".palette-empty")).toBeNull();
  });
});

describe("note rows", () => {
  const note = (title: string) => item(title, { section: "Calculator", note: true });

  it("are shown but never chosen: no highlight, no Enter, the arrows pass them by", async () => {
    await open(results(1, [note("Looking for files…"), item("Open Chrono"), item("Settings")]));
    // The first row that can be chosen is selected, not the note.
    expect($(".selected .palette-title").textContent).toBe("Open Chrono");
    expect(document.querySelector(".palette-note-row kbd")).toBeNull();
    key("ArrowDown");
    key("ArrowDown");
    expect($(".selected .palette-title").textContent).toBe("Open Chrono");
    key("ArrowUp");
    expect($(".selected .palette-title").textContent).toBe("Settings");
  });

  it("alone, leave Enter with nothing to do", async () => {
    await open(results(1, [note("Not a sum yet")]));
    expect(document.querySelector(".selected")).toBeNull();
    key("ArrowDown");
    key("Enter");
    expect(api.paletteRun).not.toHaveBeenCalled();
    expect(document.querySelector(".palette-row kbd")).toBeNull();
  });
});

describe("a time chart", () => {
  const answer = item("THE FINALS this week", { section: "Answers", sub: "Open this week in Chrono", preview: true, again: "Enter again to open this week in Chrono" });
  const card = {
    kind: "answer",
    title: "THE FINALS this week",
    icon: null,
    range: "Mon 26 Oct – Sun 1 Nov",
    category: "distracting",
    totalMs: 7_200_000,
    change: null,
    better: null,
    bars: [],
    limitMs: null,
    note: "",
    text: "2h 00m",
  };
  const subs = () => [...document.querySelectorAll(".palette-sub")].map((t) => t.textContent);

  it("waits for Enter, and the next Enter does what the row does", async () => {
    api.paletteCard.mockResolvedValue(card);
    await open(results(1, [item("THE FINALS", { section: "Apps" }), answer]));
    key("ArrowDown");
    // Selected, but no chart yet.
    await Promise.resolve();
    expect(api.paletteCard).not.toHaveBeenCalled();
    expect($("#palette-preview").hidden).toBe(true);
    key("Enter");
    await vi.waitFor(() => expect($("#palette-preview").hidden).toBe(false));
    expect(api.paletteRun).not.toHaveBeenCalled();
    expect(subs()).toContain("Enter again to open this week in Chrono");
    key("Enter");
    await vi.waitFor(() => expect(api.paletteRun).toHaveBeenCalledWith(1, 1));
  });

  it("doesn't come up as the pointer passes, and goes when another row is chosen", async () => {
    api.paletteCard.mockResolvedValue(card);
    await open(results(1, [item("THE FINALS", { section: "Apps" }), answer]));
    const rows = document.querySelectorAll<HTMLElement>("[role=option]");
    rows[1].dispatchEvent(new MouseEvent("mousemove"));
    await Promise.resolve();
    expect(api.paletteCard).not.toHaveBeenCalled();
    rows[1].click();
    await vi.waitFor(() => expect($("#palette-preview").hidden).toBe(false));
    // The rows have moved under the pointer; passing over another keeps the chart.
    rows[0].dispatchEvent(new MouseEvent("mousemove"));
    expect($("#palette-preview").hidden).toBe(false);
    key("ArrowUp");
    await vi.waitFor(() => expect($("#palette-preview").hidden).toBe(true));
    expect(subs()).toContain("Open this week in Chrono");
  });

  it("stays up when files found for the same results arrive after it", async () => {
    api.paletteCard.mockResolvedValue(card);
    await open(results(1, [item("THE FINALS", { section: "Apps" }), answer]));
    key("ArrowDown");
    key("Enter");
    await vi.waitFor(() => expect($("#palette-preview").hidden).toBe(false));
    emit("palette:files", { seq: 1, rows: [file("finals.txt")], replace: false });
    await vi.waitFor(() => expect(titles()).toContain("finals.txt"));
    expect($("#palette-preview").hidden).toBe(false);
    expect(subs()).toContain("Enter again to open this week in Chrono");
    key("Enter");
    await vi.waitFor(() => expect(api.paletteRun).toHaveBeenCalledWith(1, 1));
  });

  it("says what Enter does now even on a row with no second line", async () => {
    api.paletteCard.mockResolvedValue(card);
    await open(results(1, [item("Plain"), { ...answer, sub: null, again: "Enter again to open it" }]));
    key("ArrowDown");
    key("Enter");
    await vi.waitFor(() => expect(subs()).toContain("Enter again to open it"));
    key("ArrowUp");
    await vi.waitFor(() => expect(subs()).not.toContain("Enter again to open it"));
  });

  it("shows a day's glance as the row is selected, as before", async () => {
    api.paletteCard.mockResolvedValue({ kind: "day", title: "Yesterday", activeMs: 0, productivePct: 0, focusCount: 0, focusMs: 0, segments: [] });
    await open(results(1, [item("Yesterday", { preview: true })]));
    await vi.waitFor(() => expect(api.paletteCard).toHaveBeenCalledWith(1, 0));
  });
});
