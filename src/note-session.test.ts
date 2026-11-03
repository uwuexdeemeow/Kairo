import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Note } from "./api";
import { RETRY_AFTER_MS, SAVE_AFTER_MS, openNoteSession } from "./note-session";

const note = (body: string): Note => ({ id: "a", title: body.split("\n")[0], body, createdMs: 0, editedMs: 0, pinned: false, tasks: [] });

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
    const s = openNoteSession(box, note("Plan"), { save, syncSoon: vi.fn() }, { kept, gone: vi.fn() });
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

  it("takes a newer copy when nothing is waiting, and saves nothing once closed", async () => {
    const save = vi.fn(async (_: string, body: string) => note(body));
    const s = openNoteSession(box, note("Plan"), { save, syncSoon: vi.fn() }, { kept: vi.fn(), gone: vi.fn() });
    expect(s.take(note("Plan\nfrom Ledger"))).toBe(true);
    expect(s.editor.value()).toBe("Plan\nfrom Ledger");
    type("Plan\nlast");
    s.close();
    await vi.advanceTimersByTimeAsync(SAVE_AFTER_MS);
    expect(save).not.toHaveBeenCalled();
  });

  it("says when the note was deleted elsewhere", async () => {
    const gone = vi.fn();
    const s = openNoteSession(box, note("Plan"), { save: async () => null, syncSoon: vi.fn() }, { kept: vi.fn(), gone });
    type("Plan\nx");
    await s.save();
    expect(gone).toHaveBeenCalled();
  });
});
