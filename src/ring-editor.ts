import { ALL, DEFAULT, MAX_ITEMS, MIN_ITEMS, Ring, type Segment } from "./ring-model";
import { drawRing, mark } from "./ring-view";
import { h } from "./ui";

/// "What's on it" in the App ring's settings: every item with a tick box (on the ring or not),
/// ↑ / ↓ to order those on it, Focus's length, Reset, and a preview of the ring as it opens.
/// Each change is saved at once.

export type RingSettings = {
  shortcut: string;
  /** What's on it, by key, in order. */
  items: Segment[];
  /** Every item there is, by key, with its name. */
  all: [Segment, string][];
  focusMinutes: number;
};

export type RingEditorActions = {
  save(items: Segment[]): Promise<RingSettings>;
  setFocusMinutes(minutes: number): Promise<RingSettings>;
};

const WORDS = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten"];

export function mountRingEditor(root: HTMLElement, first: RingSettings, act: RingEditorActions): { update(s: RingSettings): void } {
  let settings = first;
  const list = h("ol", { class: "ring-choices", "aria-label": "What's on the ring" });
  const limit = h("p", { class: "muted ring-limit", "aria-live": "polite" });
  const reset = h("button", { class: "link-btn ring-reset" }, "Reset");
  const preview = h("div", { class: "ring ring-preview", "aria-hidden": "true" });
  root.replaceChildren(h("div", { class: "ring-editor" }, h("div", { class: "ring-editor-list" }, list, h("div", { class: "row-buttons" }, limit, reset)), preview));

  const nameOf = (key: Segment) => settings.all.find(([k]) => k === key)?.[1] ?? key;

  const save = async (items: Segment[]) => {
    const before = settings;
    // Shown at once; what's kept comes back.
    settings = { ...settings, items };
    draw();
    try {
      settings = await act.save(items);
      draw();
    } catch (err) {
      settings = before;
      draw();
      limit.textContent = String(err);
    }
  };

  /// Which item's control has the keyboard, so a redraw can give it back.
  const focused = (): { item: string; control: string } | null => {
    const el = document.activeElement;
    const row = el instanceof HTMLElement && list.contains(el) ? el.closest<HTMLElement>("[data-item]") : null;
    if (!row) return null;
    const control = [".ring-up", ".ring-down", ".ring-minutes"].find((c) => el!.matches(c)) ?? "input[type=checkbox]";
    return { item: row.dataset.item!, control };
  };

  /// The same item's control again; one now disabled (↑ at the top) hands over to the other
  /// arrow, then the tick box.
  const refocus = (was: { item: string; control: string }) => {
    const row = [...list.querySelectorAll<HTMLElement>("[data-item]")].find((r) => r.dataset.item === was.item);
    if (!row) return;
    for (const c of [was.control, ".ring-up", ".ring-down", "input[type=checkbox]"]) {
      const el = row.querySelector<HTMLInputElement | HTMLButtonElement>(c);
      if (el && !el.disabled) return el.focus();
    }
  };

  const draw = () => {
    const on = settings.items;
    const off = ALL.filter((k) => !on.includes(k));
    const full = on.length >= MAX_ITEMS;
    const fewest = on.length <= MIN_ITEMS;
    const was = focused();
    list.replaceChildren(
      ...[...on, ...off].map((key) => {
        const at = on.indexOf(key);
        const ticked = at >= 0;
        const box = h("input", { type: "checkbox" }) as HTMLInputElement;
        box.checked = ticked;
        box.disabled = ticked ? fewest : full;
        box.addEventListener("change", () => void save(ticked ? on.filter((k) => k !== key) : [...on, key]));
        const icon = h("span", { class: "ring-choice-mark", "aria-hidden": "true" });
        icon.innerHTML = mark(key, 22);
        const parts: Node[] = [h("label", { class: "ring-choice-name" }, box, icon, nameOf(key))];
        if (key === "focus") {
          const minutes = h("input", { class: "filter ring-minutes", type: "number", min: "5", max: "240", step: "5", "aria-label": "Focus length in minutes" }) as HTMLInputElement;
          minutes.value = String(settings.focusMinutes);
          // What's wrong with a length is said beside the field, not under the list.
          const wrong = h("span", { class: "ring-minutes-wrong", id: "ring-minutes-wrong", role: "status" });
          minutes.setAttribute("aria-describedby", "ring-minutes-wrong");
          const refuse = (why: string) => {
            minutes.value = String(settings.focusMinutes);
            minutes.setAttribute("aria-invalid", "true");
            wrong.textContent = why;
          };
          minutes.addEventListener("change", async () => {
            const n = Number(minutes.value);
            if (!minutes.value.trim() || !Number.isInteger(n) || n < 5 || n > 240) {
              refuse("Between 5 and 240 minutes");
              return;
            }
            try {
              settings = { ...(await act.setFocusMinutes(n)), items: settings.items };
              draw();
            } catch (err) {
              refuse(String(err));
            }
          });
          parts.push(h("span", { class: "ring-minutes-field" }, minutes, h("span", { class: "muted" }, "min"), wrong));
        }
        if (ticked) {
          const move = (by: number, cls: string, text: string, label: string) => {
            const b = h("button", { class: `note-tool ${cls}`, title: label, "aria-label": `${label}: ${nameOf(key)}` }, text) as HTMLButtonElement;
            const to = at + by;
            b.disabled = to < 0 || to >= on.length;
            b.addEventListener("click", () => {
              const next = [...on];
              [next[at], next[to]] = [next[to], next[at]];
              void save(next);
            });
            return b;
          };
          parts.push(h("span", { class: "ring-moves" }, move(-1, "ring-up", "↑", "Move up"), move(1, "ring-down", "↓", "Move down")));
        }
        return h("li", { class: `ring-choice${ticked ? " on" : ""}`, "data-item": key }, ...parts);
      }),
    );
    if (was) refocus(was);
    limit.textContent = full ? `The ring holds ${WORDS[MAX_ITEMS]}. Take one off to add another.` : fewest ? `At least ${WORDS[MIN_ITEMS]} stay on the ring.` : "";
    // The ring as it opens on the first item; lines are the ring's own business when it opens.
    drawRing(preview, new Ring(on, 0), { names: on.map(nameOf), lines: on.map(() => ""), focusRunning: false });
  };

  reset.addEventListener("click", () => void save([...DEFAULT]));
  draw();
  return {
    update: (s) => {
      settings = s;
      draw();
    },
  };
}
