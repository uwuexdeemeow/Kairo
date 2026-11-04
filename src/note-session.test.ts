import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Note } from "./api";
import { RETRY_AFTER_MS, SAVE_AFTER_MS, openNoteSession } from "./note-session";

const note = (body: string): Note => ({ id: "a", title: body.split("\n")[0], body, createdMs: 0, editedMs: 0, pinned: false, paper: "lined", tasks: [] });

let box: HTMLElement;
beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  document.body.innerHTML = `<div id="box"></div>`;
  box = document.getElementById("box")!;
});
afterEach(() => vi.useRealTimers());

const type = (text: string) => {
  const area = box.querySelector("textarea")!;
  area.value = text;
  area.dispatchEvent(new Event("input"));
};

describe("a note being written in", () => {
  it("keeps typing that failed to save, says so, and tries again", async () => {
    const save = vi.fn().mockRejectedValueOnce("database is locked").mockImplementation(async (_: string, body: string) => note(body));
    const kept = vi.fn();
    const s = openNoteSession(box, note("Plan"), { save, setPaper: vi.fn(), syncSoon: vi.fn() }, { kept, gone: vi.fn() });
    type("Plan\nmore");
    await vi.advanceTimersByTimeAsync(SAVE_AFTER_MS);
    expect(save).toHaveBeenCalledTimes(1);
    expect(s.dirty()).toBe(true);
    expect(s.foot.textContent).toContain("Not saved yet. Trying again…");
    // A copy from elsewhere doesn't replace what's waiting to be kept.
    expect(s.take(note("Plan\nolder"))).toBe(false);
    expect(s.editor.value()).toBe("Plan\nmore");
    await vi.advanceTimersByTimeAsync(RETRY_AFTER_MS);
    expect(save).toHaveBeenCalledTimes(2);
    expect(save).toHaveBeenLastCalledWith("a", "Plan\nmore");
    expect(s.dirty()).toBe(false);
    expect(s.foot.textContent).toContain("Saved");
    expect(kept).toHaveBeenCalledWith(note("Plan\nmore"));
  });

  it("says a failed save wasn't kept, and offers the text to copy after two", async () => {
    const save = vi.fn().mockRejectedValue("database is locked");
    const s = openNoteSession(box, note("Plan"), { save, setPaper: vi.fn(), syncSoon: vi.fn() }, { kept: vi.fn(), gone: vi.fn() });
    type("Plan\nmore");
    expect(await s.save()).toBe(false);
    expect(s.foot.querySelector(".note-state")!.classList.contains("unsaved")).toBe(true);
    expect(s.foot.querySelector(".note-copy")).toBeNull();
    expect(await s.save()).toBe(false);
    expect(s.foot.querySelector(".note-copy")!.textContent).toBe("Copy text");
    // Only whether it's kept is announced, not the word count.
    expect(s.foot.querySelector("[aria-live]")!.textContent).toBe("Not saved yet. Trying again.");
    // The state seen is not the one read out: no "Saving… Saved" at every pause.
    expect(s.foot.querySelector(".note-state")!.hasAttribute("aria-live")).toBe(false);
    expect(s.foot.hasAttribute("aria-live")).toBe(false);
  });

  it("takes a newer copy when nothing is waiting, and saves nothing once closed", async () => {
    const save = vi.fn(async (_: string, body: string) => note(body));
    const s = openNoteSession(box, note("Plan"), { save, setPaper: vi.fn(), syncSoon: vi.fn() }, { kept: vi.fn(), gone: vi.fn() });
    expect(s.take(note("Plan\nfrom Ledger"))).toBe(true);
    expect(s.editor.value()).toBe("Plan\nfrom Ledger");
    type("Plan\nlast");
    s.close();
    await vi.advanceTimersByTimeAsync(SAVE_AFTER_MS);
    expect(save).not.toHaveBeenCalled();
  });

  it("says when the note was deleted elsewhere", async () => {
    const gone = vi.fn();
    const s = openNoteSession(box, note("Plan"), { save: async () => null, setPaper: vi.fn(), syncSoon: vi.fn() }, { kept: vi.fn(), gone });
    type("Plan\nx");
    await s.save();
    expect(gone).toHaveBeenCalled();
  });

  it("draws the note on its paper, and keeps a new choice", async () => {
    const setPaper = vi.fn(async () => {});
    const kept = vi.fn();
    const s = openNoteSession(box, note("Plan"), { save: vi.fn(), setPaper, syncSoon: vi.fn() }, { kept, gone: vi.fn() });
    expect(box.dataset.paper).toBe("lined");
    const pick = s.paper.querySelector("select")!;
    expect([...pick.options].map((o) => o.text)).toEqual(["Lined", "Blank", "Dotted", "Grid"]);
    pick.value = "grid";
    pick.dispatchEvent(new Event("change"));
    expect(box.dataset.paper).toBe("grid");
    expect(setPaper).toHaveBeenCalledWith("a", "grid");
    expect(kept).toHaveBeenLastCalledWith(expect.objectContaining({ paper: "grid" }));
    // Chosen elsewhere: taken with the rest of the note.
    s.take({ ...note("Plan"), paper: "dotted" });
    expect(box.dataset.paper).toBe("dotted");
    expect(pick.value).toBe("dotted");
  });

  it("goes back to the paper it was on when the choice can't be kept", async () => {
    const setPaper = vi.fn().mockRejectedValue("database is locked");
    const s = openNoteSession(box, note("Plan"), { save: vi.fn(), setPaper, syncSoon: vi.fn() }, { kept: vi.fn(), gone: vi.fn() });
    const pick = s.paper.querySelector("select")!;
    pick.value = "blank";
    pick.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(box.dataset.paper).toBe("lined"));
    expect(pick.value).toBe("lined");
  });

  it("keeps the paper chosen while a save that set out before it comes back", async () => {
    let finish: (n: Note) => void = () => {};
    const save = vi.fn(() => new Promise<Note>((r) => (finish = r)));
    const s = openNoteSession(box, note("Plan"), { save, setPaper: vi.fn(async () => {}), syncSoon: vi.fn() }, { kept: vi.fn(), gone: vi.fn() });
    type("Plan\nmore");
    const saving = s.save();
    const pick = s.paper.querySelector("select")!;
    pick.value = "dotted";
    pick.dispatchEvent(new Event("change"));
    finish(note("Plan\nmore"));
    await saving;
    expect(s.note().paper).toBe("dotted");
    expect(box.dataset.paper).toBe("dotted");
  });
});
