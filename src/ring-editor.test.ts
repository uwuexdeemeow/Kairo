import { beforeEach, describe, expect, it, vi } from "vitest";
import { mountRingEditor, type RingEditorActions, type RingSettings } from "./ring-editor";
import { ALL, DEFAULT, type Segment } from "./ring-model";

const NAMES: Record<Segment, string> = {
  chrono: "Chrono",
  ledger: "Ledger",
  palette: "Palette",
  dashboard: "Dashboard",
  task: "New task",
  note: "New note",
  focus: "Focus 25",
  notes: "All notes",
  week: "This week",
  settings: "Settings",
  pause: "Pause tracking",
};

let root: HTMLElement;
let act: { [K in keyof RingEditorActions]: ReturnType<typeof vi.fn> };
const settings = (items: Segment[], focusMinutes = 25): RingSettings => ({
  shortcut: "Alt+Q",
  items,
  all: ALL.map((k) => [k, k === "focus" ? `Focus ${focusMinutes}` : NAMES[k]]),
  focusMinutes,
});

beforeEach(() => {
  document.body.innerHTML = `<div id="root"></div>`;
  root = document.getElementById("root")!;
  act = {
    save: vi.fn(async (items: Segment[]) => settings(items)),
    setFocusMinutes: vi.fn(async (m: number) => settings([...DEFAULT], m)),
  };
});

const rows = () => [...root.querySelectorAll<HTMLElement>(".ring-choice")];
const row = (key: Segment) => root.querySelector<HTMLElement>(`.ring-choice[data-item="${key}"]`)!;
const box = (key: Segment) => row(key).querySelector<HTMLInputElement>("input[type=checkbox]")!;
const order = () => rows().map((r) => r.dataset.item);
const onRing = () => rows().filter((r) => r.querySelector<HTMLInputElement>("input[type=checkbox]")!.checked).map((r) => r.dataset.item);

describe("arranging the ring", () => {
  it("lists what's on it first, in order, then the rest", () => {
    mountRingEditor(root, settings(["week", "chrono", "notes"]), act);
    expect(order()).toEqual(["week", "chrono", "notes", "ledger", "palette", "dashboard", "task", "note", "focus", "settings", "pause"]);
    expect(onRing()).toEqual(["week", "chrono", "notes"]);
    expect(row("week").textContent).toContain("This week");
  });

  it("adds a ticked item at the end and takes an unticked one off, saving each", async () => {
    mountRingEditor(root, settings([...DEFAULT]), act);
    box("week").click();
    await vi.waitFor(() => expect(act.save).toHaveBeenLastCalledWith([...DEFAULT, "week"]));
    expect(onRing()).toEqual([...DEFAULT, "week"]);
    box("palette").click();
    await vi.waitFor(() => expect(act.save).toHaveBeenLastCalledWith(["chrono", "ledger", "dashboard", "task", "note", "focus", "week"]));
  });

  it("moves an item up and down among those on it", async () => {
    mountRingEditor(root, settings(["chrono", "ledger", "notes"]), act);
    row("notes").querySelector<HTMLButtonElement>(".ring-up")!.click();
    await vi.waitFor(() => expect(act.save).toHaveBeenLastCalledWith(["chrono", "notes", "ledger"]));
    expect(order().slice(0, 3)).toEqual(["chrono", "notes", "ledger"]);
    row("notes").querySelector<HTMLButtonElement>(".ring-down")!.click();
    await vi.waitFor(() => expect(act.save).toHaveBeenLastCalledWith(["chrono", "ledger", "notes"]));
    // The first can't go up, the last on the ring can't go down; those off it have no arrows.
    expect(row("chrono").querySelector<HTMLButtonElement>(".ring-up")!.disabled).toBe(true);
    expect(row("notes").querySelector<HTMLButtonElement>(".ring-down")!.disabled).toBe(true);
    expect(row("week").querySelector(".ring-up")).toBeNull();
  });

  it("keeps the keyboard on the item being moved, through each save", async () => {
    mountRingEditor(root, settings(["chrono", "ledger", "notes"]), act);
    row("notes").querySelector<HTMLButtonElement>(".ring-up")!.focus();
    row("notes").querySelector<HTMLButtonElement>(".ring-up")!.click();
    await vi.waitFor(() => expect(act.save).toHaveBeenLastCalledWith(["chrono", "notes", "ledger"]));
    expect(document.activeElement).toBe(row("notes").querySelector(".ring-up"));
    // Moved to the top, ↑ is disabled: the keyboard goes to ↓ instead of nowhere.
    row("notes").querySelector<HTMLButtonElement>(".ring-up")!.click();
    await vi.waitFor(() => expect(act.save).toHaveBeenLastCalledWith(["notes", "chrono", "ledger"]));
    expect(document.activeElement).toBe(row("notes").querySelector(".ring-down"));
  });

  it("keeps between three and ten, and says why", () => {
    mountRingEditor(root, settings(["chrono", "ledger", "notes"]), act);
    expect(box("chrono").disabled).toBe(true);
    expect(box("week").disabled).toBe(false);
    expect(root.querySelector(".ring-limit")!.textContent).toBe("At least three stay on the ring.");
    mountRingEditor(root, settings(ALL.slice(0, 10)), act);
    expect(box("pause").disabled).toBe(true);
    expect(box("chrono").disabled).toBe(false);
    expect(root.querySelector(".ring-limit")!.textContent).toBe("The ring holds ten. Take one off to add another.");
  });

  it("goes back to the first seven with Reset", async () => {
    mountRingEditor(root, settings(["week", "notes", "pause"]), act);
    root.querySelector<HTMLButtonElement>(".ring-reset")!.click();
    await vi.waitFor(() => expect(act.save).toHaveBeenLastCalledWith([...DEFAULT]));
    expect(onRing()).toEqual([...DEFAULT]);
    // Your own order can come back.
    await vi.waitFor(() => expect(root.querySelector(".ring-limit")!.textContent).toBe("Reset to Kairo's order · Undo"));
    root.querySelector<HTMLButtonElement>(".ring-undo")!.click();
    await vi.waitFor(() => expect(act.save).toHaveBeenLastCalledWith(["week", "notes", "pause"]));
  });

  it("sets Focus's length beside its row", async () => {
    mountRingEditor(root, settings([...DEFAULT]), act);
    const minutes = row("focus").querySelector<HTMLInputElement>(".ring-minutes")!;
    expect(minutes.value).toBe("25");
    minutes.value = "45";
    minutes.dispatchEvent(new Event("change"));
    await vi.waitFor(() => expect(act.setFocusMinutes).toHaveBeenCalledWith(45));
    await vi.waitFor(() => expect(row("focus").textContent).toContain("Focus 45"));
  });

  it("says beside the field when a length can't be used", async () => {
    mountRingEditor(root, settings([...DEFAULT]), act);
    const minutes = row("focus").querySelector<HTMLInputElement>(".ring-minutes")!;
    minutes.value = "2";
    minutes.dispatchEvent(new Event("change"));
    expect(act.setFocusMinutes).not.toHaveBeenCalled();
    expect(minutes.value).toBe("25");
    expect(minutes.getAttribute("aria-invalid")).toBe("true");
    expect(row("focus").querySelector(".ring-minutes-wrong")!.textContent).toBe("Between 5 and 240 minutes");
  });

  it("draws a preview of the ring, redrawn as it changes", async () => {
    mountRingEditor(root, settings(["notes", "week", "pause"]), act);
    const segs = () => [...root.querySelectorAll(".ring-preview .ring-seg")].map((s) => s.getAttribute("data-segment")).sort();
    expect(segs()).toEqual(["notes", "pause", "week"]);
    expect(root.querySelector(".ring-preview .ring-name")!.textContent).toBe("All notes");
    box("chrono").click();
    await vi.waitFor(() => expect(segs()).toEqual(["chrono", "notes", "pause", "week"]));
  });

  it("says why when saving is refused, and shows what's kept", async () => {
    act.save.mockRejectedValueOnce("The ring holds 3 to 10 items.");
    mountRingEditor(root, settings([...DEFAULT]), act);
    box("week").click();
    await vi.waitFor(() => expect(root.querySelector(".ring-limit")!.textContent).toBe("The ring holds 3 to 10 items."));
    expect(onRing()).toEqual([...DEFAULT]);
  });
});
