import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Note } from "./api";
import { editedText, mountNotes, type NotesActions } from "./ledger-notes";

const NOW = new Date(2026, 10, 3, 15, 0).getTime();
const note = (id: string, body: string, extra: Partial<Note> = {}): Note => ({
  id,
  title: body.split("\n")[0].replace(/^#+\s*/, "") || "New note",
  body,
  createdMs: NOW,
  editedMs: NOW,
  pinned: false,
  tasks: [],
  ...extra,
});

let root: HTMLElement;
let notes: Note[];
let act: { [K in keyof NotesActions]: ReturnType<typeof vi.fn> };

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  document.body.innerHTML = `<div id="root"></div>`;
  root = document.getElementById("root")!;
  notes = [note("a", "# Plan\nintro", { pinned: true, tasks: [["t1", "Essay"], ["t2", "Talk"]] }), note("b", "Shopping")];
  act = {
    search: vi.fn(async (q: string) => notes.filter((n) => n.body.toLowerCase().includes(q.toLowerCase()))),
    create: vi.fn(async () => note("new", "")),
    save: vi.fn(async (id: string, body: string) => note(id, body)),
    pin: vi.fn(async () => {}),
    remove: vi.fn(async () => {}),
    openTask: vi.fn(),
    syncSoon: vi.fn(),
  };
});

afterEach(() => vi.useRealTimers());

const rows = () => [...root.querySelectorAll(".note-row")].map((r) => r.querySelector(".note-row-title")!.textContent);
const settle = () => vi.waitFor(() => expect(act.search).toHaveBeenCalled());

describe("when a note was edited", () => {
  it("reads as a time today, Yesterday, or a date", () => {
    expect(editedText(new Date(2026, 10, 3, 9, 14).getTime(), NOW)).toMatch(/^0?9:14/);
    expect(editedText(new Date(2026, 10, 2, 22, 0).getTime(), NOW)).toBe("Yesterday");
    expect(editedText(new Date(2026, 9, 28).getTime(), NOW)).toBe("28 Oct");
  });
});

describe("the notes list", () => {
  it("lists notes as the search gives them, with attached tasks counted", async () => {
    mountNotes(root, act, () => NOW);
    await settle();
    await vi.waitFor(() => expect(rows()).toEqual(["Plan", "Shopping"]));
    expect(root.querySelector('[data-note="a"] .note-row-meta')!.textContent).toContain("2 tasks");
    expect(root.querySelector('[data-note="a"].pinned')).not.toBeNull();
  });

  it("says what to write with no notes", async () => {
    notes = [];
    mountNotes(root, act, () => NOW);
    await vi.waitFor(() => expect(root.querySelector(".notes-empty")!.textContent).toBe("Write anything: plans, lists, things to remember."));
  });

  it("searches as you type", async () => {
    mountNotes(root, act, () => NOW);
    await settle();
    const search = root.querySelector<HTMLInputElement>(".notes-search")!;
    search.value = "shop";
    search.dispatchEvent(new Event("input"));
    await vi.waitFor(() => expect(rows()).toEqual(["Shopping"]));
    expect(act.search).toHaveBeenLastCalledWith("shop");
  });

  it("makes a new note and opens it", async () => {
    mountNotes(root, act, () => NOW);
    await settle();
    root.querySelector<HTMLButtonElement>(".notes-new")!.click();
    await vi.waitFor(() => expect(root.querySelector(".note-pane textarea")).not.toBeNull());
    expect(act.create).toHaveBeenCalled();
  });
});

describe("an open note", () => {
  async function opened() {
    const view = mountNotes(root, act, () => NOW);
    await settle();
    await view.open("a");
    return { view, area: root.querySelector<HTMLTextAreaElement>(".note-pane textarea")! };
  }

  it("shows its text and the tasks it's attached to", async () => {
    const { area } = await opened();
    expect(area.value).toBe("# Plan\nintro");
    const links = [...root.querySelectorAll<HTMLButtonElement>(".note-tasks button")];
    expect(links.map((l) => l.textContent)).toEqual(["Essay ↗", "Talk ↗"]);
    links[1].click();
    expect(act.openTask).toHaveBeenCalledWith("t2");
  });

  it("saves half a second after typing stops, and asks for a sync after 30", async () => {
    const { area } = await opened();
    area.value = "# Plan\nintro more";
    area.dispatchEvent(new Event("input"));
    vi.advanceTimersByTime(400);
    expect(act.save).not.toHaveBeenCalled();
    vi.advanceTimersByTime(100);
    expect(act.save).toHaveBeenCalledWith("a", "# Plan\nintro more");
    vi.advanceTimersByTime(29_000);
    expect(act.syncSoon).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1_000);
    expect(act.syncSoon).toHaveBeenCalledTimes(1);
  });

  it("saves at once when another note is opened", async () => {
    const { view, area } = await opened();
    area.value = "changed";
    area.dispatchEvent(new Event("input"));
    await view.open("b");
    expect(act.save).toHaveBeenCalledWith("a", "changed");
  });

  it("pins and unpins", async () => {
    await opened();
    const pin = root.querySelector<HTMLButtonElement>(".note-pin")!;
    expect(pin.textContent).toBe("Unpin");
    pin.click();
    expect(act.pin).toHaveBeenCalledWith("a", false);
  });

  it("asks in place before deleting, and Keep leaves it", async () => {
    await opened();
    root.querySelector<HTMLButtonElement>(".note-delete")!.click();
    expect(root.querySelector(".note-confirm")!.textContent).toContain("Delete “Plan”? Tasks attached to it lose the link.");
    root.querySelector<HTMLButtonElement>(".note-keep")!.click();
    expect(root.querySelector(".note-confirm")).toBeNull();
    expect(act.remove).not.toHaveBeenCalled();
    root.querySelector<HTMLButtonElement>(".note-delete")!.click();
    root.querySelector<HTMLButtonElement>(".note-confirm .btn")!.click();
    await vi.waitFor(() => expect(act.remove).toHaveBeenCalledWith("a"));
    await vi.waitFor(() => expect(root.querySelector(".note-pane textarea")).toBeNull());
  });

  it("asks only once however often Delete is pressed", async () => {
    await opened();
    const del = root.querySelector<HTMLButtonElement>(".note-delete")!;
    del.click();
    del.click();
    del.click();
    expect(root.querySelectorAll(".note-confirm").length).toBe(1);
    const yes = root.querySelector<HTMLButtonElement>(".note-confirm .btn")!;
    yes.click();
    yes.click();
    await vi.waitFor(() => expect(act.remove).toHaveBeenCalled());
    expect(act.remove).toHaveBeenCalledTimes(1);
  });

  it("takes a newer text from elsewhere unless typing", async () => {
    const { view, area } = await opened();
    notes[0] = note("a", "# Plan\nfrom the laptop", { pinned: true });
    await view.refresh();
    expect(area.value).toBe("# Plan\nfrom the laptop");
    area.value = "mine";
    area.dispatchEvent(new Event("input"));
    notes[0] = note("a", "# Plan\nagain", { pinned: true });
    await view.refresh();
    expect(area.value).toBe("mine");
  });

  it("says when a note is at its limit", async () => {
    const { area } = await opened();
    area.value = "x".repeat(100_000);
    area.dispatchEvent(new Event("input"));
    expect(root.querySelector(".note-limit")!.textContent).toBe("This note is at its limit.");
  });
});
