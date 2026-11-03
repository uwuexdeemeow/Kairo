import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Note } from "./api";
import { mountPopout, type PopoutActions } from "./note-popout";

const note = (id: string, body: string): Note => ({
  id,
  title: body.split("\n")[0].replace(/^#+\s*/, "") || "New note",
  body,
  createdMs: 0,
  editedMs: 0,
  pinned: false,
  tasks: [],
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
    save: vi.fn(async (id: string, body: string) => (kept[id] ? (kept[id] = note(id, body)) : null)),
    setTitle: vi.fn(),
    close: vi.fn(),
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

describe("the popped-out note", () => {
  it("shows the note it's given, its title on the window", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    expect(area().value).toBe("# Plan\n- [ ] intro");
    expect(act.setTitle).toHaveBeenCalledWith("Plan");
    expect(foot()).toContain("Saved");
    expect(foot()).toContain("2 words");
  });

  it("closes when the note can't be found", async () => {
    const pop = mountPopout(root, act);
    await pop.show("gone");
    expect(act.close).toHaveBeenCalled();
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
    await pop.changed({ id: "a", from: "note", gone: false });
    await pop.changed({ id: "b", from: "kairo", gone: false });
    expect(act.load).not.toHaveBeenCalled();
  });

  it("closes when the note is deleted, here or on another computer", async () => {
    const pop = mountPopout(root, act);
    await pop.show("a");
    await pop.changed({ id: "a", from: "kairo", gone: true });
    expect(act.close).toHaveBeenCalledTimes(1);
    // After a sync, a note that's no longer there.
    await pop.show("b");
    delete kept.b;
    await pop.refresh();
    expect(act.close).toHaveBeenCalledTimes(2);
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
