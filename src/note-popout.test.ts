import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Note } from "./api";
import { mountPopout, type PopoutActions } from "./note-popout";

const note = (id: string, body: string, extra: Partial<Note> = {}): Note => ({
  id,
  title: body.split("\n")[0].replace(/^#+\s*/, "") || "New note",
  body,
  createdMs: 0,
  editedMs: 0,
  pinned: false,
  paper: "lined",
  tasks: [],
  ...extra,
});

let root: HTMLElement;
let kept: Record<string, Note>;
let act: { [K in keyof PopoutActions]: ReturnType<typeof vi.fn> };

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  document.body.innerHTML = `<div id="root"></div>`;
  root = document.getElementById("root")!;
  kept = { a: note("a", "# Plan\n- [ ] intro"), b: note("b", "Shopping") };
  act = {
    load: vi.fn(async (id: string) => kept[id] ?? null),
    search: vi.fn(async (q: string) => Object.values(kept).filter((n) => n.body.toLowerCase().includes(q.toLowerCase()))),
    create: vi.fn(async () => (kept.new = note("new", ""))),
    save: vi.fn(async (id: string, body: string) => (kept[id] ? (kept[id] = note(id, body)) : null)),
    setPaper: vi.fn(async () => {}),
    setTitle: vi.fn(),
    onTop: vi.fn(async () => true),
    setOnTop: vi.fn(async () => {}),
    openInLedger: vi.fn(),
    syncSoon: vi.fn(),
  };
});

afterEach(() => vi.useRealTimers());

const area = () => root.querySelector("textarea")!;
const type = (text: string) => {
  area().value = text;
  area().dispatchEvent(new Event("input"));
};
const foot = () => root.querySelector(".note-foot")!.textContent;
const rows = () => [...root.querySelectorAll(".popout-row strong")].map((r) => r.textContent);

describe("the popped-out note", () => {
  it("shows the note it's given, its title on the window", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    expect(area().value).toBe("# Plan\n- [ ] intro");
    expect(act.setTitle).toHaveBeenCalledWith("Plan");
    expect(foot()).toContain("Saved");
    expect(foot()).toContain("2 words");
  });

  it("shows the list when the note can't be found", async () => {
    const pop = mountPopout(root, act);
    await pop.show("gone");
    await vi.waitFor(() => expect(rows()).toEqual(["Plan", "Shopping"]));
  });

  it("saves a moment after typing stops, and retitles the window", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    type("# Essay\n- [ ] intro");
    expect(foot()).toContain("Saving…");
    expect(act.save).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(500);
    expect(act.save).toHaveBeenCalledWith("a", "# Essay\n- [ ] intro");
    expect(act.setTitle).toHaveBeenLastCalledWith("Essay");
    expect(foot()).toContain("Saved");
  });

  it("saves at once when asked, as when the window loses focus", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    type("# Plan\nmore");
    await pop.flush();
    expect(act.save).toHaveBeenCalledWith("a", "# Plan\nmore");
  });

  it("takes a change made in another window when nothing is unsaved, keeping the caret", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    area().setSelectionRange(3, 3);
    kept.a = note("a", "# Plan\n- [x] intro");
    await pop.changed({ id: "a", from: "kairo", gone: false });
    expect(area().value).toBe("# Plan\n- [x] intro");
    expect(area().selectionStart).toBe(3);
  });

  it("keeps its own typing over a change from elsewhere", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    type("# Plan\nmine");
    kept.a = note("a", "# Plan\ntheirs");
    await pop.changed({ id: "a", from: "kairo", gone: false });
    expect(area().value).toBe("# Plan\nmine");
  });

  it("ignores its own changes and other notes'", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    act.load.mockClear();
    act.search.mockClear();
    await pop.changed({ id: "a", from: "note", gone: false });
    await pop.changed({ id: "b", from: "kairo", gone: false });
    expect(act.load).not.toHaveBeenCalled();
    expect(act.search).not.toHaveBeenCalled();
  });

  it("goes back to the list when the note is deleted, here or on another computer", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    delete kept.a;
    await pop.changed({ id: "a", from: "kairo", gone: true });
    await vi.waitFor(() => expect(rows()).toEqual(["Shopping"]));
    // After a sync, a note that's no longer there.
    await pop.show("b");
    delete kept.b;
    await pop.refresh();
    await vi.waitFor(() => expect(root.querySelector(".popout-list")).not.toBeNull());
  });

  it("stays on a note whose save is failing rather than lose its typing", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    act.save.mockRejectedValue("database is locked");
    type("# Plan\nnot kept");
    await pop.show("b");
    expect(area().value).toBe("# Plan\nnot kept");
    await pop.list();
    expect(root.querySelector(".popout-list")).toBeNull();
    expect(area().value).toBe("# Plan\nnot kept");
  });

  it("saves the note it shows before switching to another", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    type("# Plan\nunsaved");
    await pop.show("b");
    expect(act.save).toHaveBeenCalledWith("a", "# Plan\nunsaved");
    expect(area().value).toBe("Shopping");
    expect(act.setTitle).toHaveBeenLastCalledWith("Shopping");
  });

  it("floats on top as set, and its button turns it off and on", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    const pin = root.querySelector<HTMLButtonElement>('[data-tool="on-top"]')!;
    await vi.waitFor(() => expect(pin.getAttribute("aria-pressed")).toBe("true"));
    pin.click();
    await vi.waitFor(() => expect(act.setOnTop).toHaveBeenCalledWith(false));
    expect(pin.getAttribute("aria-pressed")).toBe("false");
    pin.click();
    await vi.waitFor(() => expect(act.setOnTop).toHaveBeenLastCalledWith(true));
  });

  it("opens the note in Ledger, saving first", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    type("# Plan\nlast words");
    root.querySelector<HTMLButtonElement>('[data-tool="ledger"]')!.click();
    await vi.waitFor(() => expect(act.openInLedger).toHaveBeenCalledWith("a"));
    expect(act.save).toHaveBeenCalledWith("a", "# Plan\nlast words");
  });

  it("has bold, italic and checkbox tools", async () => {
    const pop = mountPopout(root, act);
    await pop.show("b");
    area().setSelectionRange(0, 8);
    root.querySelector<HTMLButtonElement>('[data-tool="bold"]')!.click();
    expect(area().value).toBe("**Shopping**");
    root.querySelector<HTMLButtonElement>('[data-tool="box"]')!.click();
    expect(area().value).toBe("- [ ] **Shopping**");
  });
});

describe("the list of notes in the pop-out", () => {
  it("lists the notes, titled Notes, with each one's text under its title", async () => {
    kept.a = note("a", "# Plan\n- [ ] intro", { pinned: true });
    const pop = mountPopout(root, act);
    await pop.list();
    await vi.waitFor(() => expect(rows()).toEqual(["Plan", "Shopping"]));
    expect(act.setTitle).toHaveBeenLastCalledWith("Notes");
    expect(root.querySelector('.popout-row[data-note="a"]')!.classList.contains("pinned")).toBe(true);
    expect(root.querySelector('.popout-row[data-note="a"] .popout-row-preview')!.textContent).toBe("intro");
  });

  it("keeps each note a button inside its list item, for screen readers", async () => {
    const pop = mountPopout(root, act);
    await pop.list();
    await vi.waitFor(() => expect(rows().length).toBe(2));
    const items = [...root.querySelectorAll('.popout-list > [role="listitem"]')];
    expect(items.length).toBe(2);
    for (const item of items) {
      const button = item.querySelector("button.popout-row")!;
      expect(button.hasAttribute("role")).toBe(false);
    }
  });

  it("searches as you type", async () => {
    const pop = mountPopout(root, act);
    await pop.list();
    const search = root.querySelector<HTMLInputElement>(".popout-search")!;
    search.value = "shop";
    search.dispatchEvent(new Event("input"));
    await vi.waitFor(() => expect(rows()).toEqual(["Shopping"]));
  });

  it("opens a note from the list, and ☰ goes back to it", async () => {
    const pop = mountPopout(root, act);
    await pop.list();
    await vi.waitFor(() => expect(rows().length).toBe(2));
    root.querySelector<HTMLButtonElement>('.popout-row[data-note="b"]')!.click();
    await vi.waitFor(() => expect(root.querySelector("textarea")?.value).toBe("Shopping"));
    expect(act.setTitle).toHaveBeenLastCalledWith("Shopping");
    type("Shopping\nmilk");
    root.querySelector<HTMLButtonElement>('[data-tool="list"]')!.click();
    await vi.waitFor(() => expect(root.querySelector(".popout-list")).not.toBeNull());
    // What was typed is kept on the way back.
    expect(act.save).toHaveBeenCalledWith("b", "Shopping\nmilk");
    expect(act.setTitle).toHaveBeenLastCalledWith("Notes");
  });

  it("makes a new note and opens it, ready to type", async () => {
    const pop = mountPopout(root, act);
    await pop.list();
    root.querySelector<HTMLButtonElement>(".popout-new")!.click();
    await vi.waitFor(() => expect(act.create).toHaveBeenCalled());
    await vi.waitFor(() => expect(root.querySelector("textarea")).not.toBeNull());
    expect(document.activeElement).toBe(area());
  });

  it("goes through the list from the keyboard: Enter opens the top match, the arrows move", async () => {
    const pop = mountPopout(root, act);
    await pop.list();
    await vi.waitFor(() => expect(rows().length).toBe(2));
    const search = root.querySelector<HTMLInputElement>(".popout-search")!;
    const press = (el: Element, key: string) => el.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }));
    press(search, "ArrowDown");
    expect(document.activeElement).toBe(root.querySelectorAll(".popout-row")[0]);
    press(document.activeElement!, "ArrowDown");
    expect(document.activeElement).toBe(root.querySelectorAll(".popout-row")[1]);
    press(document.activeElement!, "ArrowUp");
    press(document.activeElement!, "ArrowUp");
    expect(document.activeElement).toBe(search);
    search.value = "shop";
    search.dispatchEvent(new Event("input"));
    await vi.waitFor(() => expect(rows()).toEqual(["Shopping"]));
    press(search, "Enter");
    await vi.waitFor(() => expect(area().value).toBe("Shopping"));
    // Esc goes back to the list.
    press(area(), "Escape");
    await vi.waitFor(() => expect(root.querySelector(".popout-search")).not.toBeNull());
  });

  it("starts a note with Ctrl+N, titled with what was searched for", async () => {
    const pop = mountPopout(root, act);
    await pop.list();
    const search = root.querySelector<HTMLInputElement>(".popout-search")!;
    search.value = "Groceries";
    search.dispatchEvent(new KeyboardEvent("keydown", { key: "n", ctrlKey: true, bubbles: true, cancelable: true }));
    await vi.waitFor(() => expect(act.create).toHaveBeenCalledWith("Groceries\n"));
    await vi.waitFor(() => expect(root.querySelector("textarea")).not.toBeNull());
  });

  it("says when a note couldn't be started, read or opened, keeping the list", async () => {
    const pop = mountPopout(root, act);
    act.search.mockRejectedValueOnce(new Error("busy"));
    await pop.list();
    await vi.waitFor(() => expect(root.textContent).toContain("Couldn't read your notes."));
    root.querySelector<HTMLButtonElement>(".popout-empty .link-btn")!.click();
    await vi.waitFor(() => expect(rows().length).toBe(2));
    act.create.mockRejectedValueOnce(new Error("busy"));
    root.querySelector<HTMLButtonElement>(".popout-new")!.click();
    await vi.waitFor(() => expect(root.querySelector("[role=status]")!.textContent).toBe("Couldn't start a note. Try again."));
    act.load.mockRejectedValueOnce(new Error("busy"));
    root.querySelector<HTMLButtonElement>(".popout-row")!.click();
    await vi.waitFor(() => expect(root.querySelector("[role=status]")!.textContent).toBe("Couldn't open that note. Try again."));
    expect(rows().length).toBe(2);
  });

  it("tells screen readers which notes are pinned", async () => {
    kept.a = note("a", "# Plan", { pinned: true });
    const pop = mountPopout(root, act);
    await pop.list();
    await vi.waitFor(() => expect(rows().length).toBe(2));
    expect(root.querySelector(".popout-row.pinned")!.textContent).toContain(", pinned");
  });

  it("says what to do with no notes", async () => {
    kept = {};
    const pop = mountPopout(root, act);
    await pop.list();
    await vi.waitFor(() => expect(root.querySelector(".popout-list")!.textContent).toContain("No notes yet."));
  });

  it("reads the list again when a note changes elsewhere", async () => {
    const pop = mountPopout(root, act);
    await pop.list();
    await vi.waitFor(() => expect(rows().length).toBe(2));
    kept.c = note("c", "Ideas");
    await pop.changed({ id: "c", from: "kairo", gone: false });
    await vi.waitFor(() => expect(rows()).toContain("Ideas"));
  });

  it("shows the note on its paper, with the Paper choice in its bar", async () => {
    kept.a = { ...kept.a, paper: "grid" };
    const pop = mountPopout(root, act);
    await pop.show("a");
    expect(root.querySelector<HTMLElement>(".popout-body")!.dataset.paper).toBe("grid");
    const pick = root.querySelector<HTMLSelectElement>(".popout-bar .note-paper select")!;
    pick.value = "blank";
    pick.dispatchEvent(new Event("change"));
    expect(act.setPaper).toHaveBeenCalledWith("a", "blank");
  });
});
