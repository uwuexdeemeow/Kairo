// Times of day picked from Kairo's own dropdown (dropdown.ts) in place of the browser's time
// field, which ignores the app's theme: a choice every quarter hour, written as this computer
// writes times. The select's value stays "HH:MM", like the field's, so code reading and setting it
// doesn't change; a time off the quarter hours becomes its own choice rather than none.

/// Minutes between the times offered.
const STEP = 15;

const pad = (n: number) => String(n).padStart(2, "0");

/// "15:00" as this computer writes a time of day, like "3:00 PM".
export function timeLabel(hhmm: string): string {
  const [hh, mm] = hhmm.split(":").map(Number);
  return new Date(2000, 0, 1, hh, mm).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

const isTime = (v: string) => /^\d{2}:\d{2}$/.test(v);

/// Adds `value` as a choice, in order, when it isn't one already.
function offer(select: HTMLSelectElement, value: string) {
  if (!isTime(value) || [...select.options].some((o) => o.value === value)) return;
  const option = document.createElement("option");
  option.value = value;
  option.textContent = timeLabel(value);
  const after = [...select.options].find((o) => o.value > value);
  select.insertBefore(option, after ?? null);
}

/// A choice of times every quarter hour with `value` chosen. Setting its value later to a time
/// off the quarter hours adds that time first, so it's never left blank.
export function timeSelect(value: string, label?: string): HTMLSelectElement {
  const select = document.createElement("select");
  for (let m = 0; m < 24 * 60; m += STEP) {
    const t = `${pad(Math.floor(m / 60))}:${pad(m % 60)}`;
    const option = document.createElement("option");
    option.value = t;
    option.textContent = timeLabel(t);
    select.append(option);
  }
  if (label) select.setAttribute("aria-label", label);
  select.dataset.times = "";
  offerOffGrid();
  select.value = isTime(value) ? value : "09:00";
  return select;
}

let offering = false;

/// Once: setting a time dropdown's value (marked `data-times`) to a time it doesn't offer adds it
/// first. The setter is wrapped as dropdown.ts wraps it, so either order keeps both.
function offerOffGrid() {
  if (offering) return;
  offering = true;
  const desc = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value");
  if (!desc?.set) return;
  Object.defineProperty(HTMLSelectElement.prototype, "value", {
    ...desc,
    set(this: HTMLSelectElement, v: unknown) {
      if (this.dataset.times !== undefined) offer(this, String(v));
      desc.set!.call(this, v);
    },
  });
}

/// Puts a time dropdown in place of each time field under `root`, keeping its id, name, class,
/// label and value.
export function replaceTimeInputs(root: ParentNode = document) {
  for (const input of root.querySelectorAll<HTMLInputElement>('input[type="time"]')) {
    const select = timeSelect(input.value);
    for (const attr of [...input.attributes]) {
      if (attr.name !== "type" && attr.name !== "value") select.setAttribute(attr.name, attr.value);
    }
    input.replaceWith(select);
  }
}
