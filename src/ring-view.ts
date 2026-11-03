/// The web ring's page (macOS, Linux): the arc of segments, the lit one at the top with its logo,
/// name and line, and everything that turns it or picks from it. Kairo's settings draw their
/// preview of the ring with the same `drawRing`.
import { logo } from "./logos";
import { CENTRE, DRAG_PX, HEIGHT, INNER, OUTER, Ring, STEP, WIDTH, angle, dragSteps, onModifierUp, segmentPath, shown, type Segment } from "./ring-model";

export type RingState = { items: Segment[]; lit: number; names: string[]; lines: string[]; focusRunning: boolean; paused?: boolean };
export type RingActions = { pick(index: number): void; close(): void };
export type RingView = { turn(steps: number): void; lit(): number };

/// The quick actions' marks, drawn like the logos (and as the Windows ring draws them).
const MARKS: Record<Exclude<Segment, "chrono" | "ledger" | "palette" | "dashboard">, string> = {
  task: `<path d="M50 22v56M22 50h56" style="stroke:var(--ledger)" stroke-width="10" stroke-linecap="round"/>`,
  note: `<path d="M28 74l6-20 34-34 14 14-34 34z" style="fill:none;stroke:var(--ledger)" stroke-width="7" stroke-linejoin="round"/><path d="M28 80h44" style="stroke:var(--ink)" stroke-width="5" stroke-linecap="round" opacity=".6"/>`,
  focus: `<circle cx="50" cy="54" r="32" style="fill:none;stroke:var(--chrono)" stroke-width="7"/><path d="M50 54V36M42 14h16" style="stroke:var(--ink)" stroke-width="7" stroke-linecap="round"/>`,
  notes: `<rect x="32" y="12" width="50" height="62" rx="6" style="fill:none;stroke:var(--ledger)" stroke-width="5" opacity=".55"/><rect x="18" y="26" width="50" height="62" rx="6" style="fill:var(--raised);stroke:var(--ledger)" stroke-width="5"/><path d="M30 44h26M30 56h26M30 68h16" style="stroke:var(--ledger)" stroke-width="4" stroke-linecap="round"/>`,
  week: `<path d="M17 82V52M28 82V36M39 82V44M50 82V24M61 82V56M72 82V32M83 82V62" style="stroke:var(--chrono)" stroke-width="7" stroke-linecap="round"/>`,
  settings: `<path d="M16 28h68M16 50h68M16 72h68" style="stroke:var(--ink)" stroke-width="5" stroke-linecap="round"/><circle cx="64" cy="28" r="9" style="fill:var(--ribbon)"/><circle cx="34" cy="50" r="9" style="fill:var(--ribbon)"/><circle cx="56" cy="72" r="9" style="fill:var(--ribbon)"/>`,
  pause: `<path d="M38 24v52M62 24v52" style="stroke:var(--chrono)" stroke-width="13" stroke-linecap="round"/>`,
};
/// Pause while tracking is paused: a triangle, to start again.
const RESUME = `<path d="M32 22l46 28-46 28z" style="fill:var(--chrono)"/>`;

export function mark(segment: Segment, size: number, paused = false): string {
  switch (segment) {
    case "chrono":
    case "ledger":
    case "palette":
      return logo(segment, size);
    case "dashboard":
      return logo("kairo", size);
    default:
      return `<svg class="logo" width="${size}" height="${size}" viewBox="0 0 100 100" aria-hidden="true">${segment === "pause" && paused ? RESUME : MARKS[segment]}</svg>`;
  }
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/// Where a segment's middle is, `offset` places from the top.
function middle(offset: number): { x: number; y: number } {
  const r = (OUTER + INNER) / 2 + (offset === 0 ? 8 : 0);
  const a = (offset * STEP * Math.PI) / 180;
  return { x: CENTRE.x + r * Math.sin(a), y: CENTRE.y - r * Math.cos(a) };
}

/// Draws `ring` into `root`: the segments that show, the neighbours' marks, and the lit one's
/// mark, name and line.
export function drawRing(root: HTMLElement, ring: Ring, state: Pick<RingState, "names" | "lines" | "focusRunning" | "paused">): void {
  // Back to front, so the lit one's outline sits on top.
  const offsets = shown(ring.items.length).sort((a, b) => Math.abs(b) - Math.abs(a));
  const segs = offsets
    .map((offset) => {
      const segment = ring.at(offset);
      const lit = offset === 0;
      const dim = segment === "focus" && state.focusRunning;
      return `<path class="ring-seg seg-${segment} off${Math.abs(offset)}${lit ? " lit" : ""}${dim ? " dim" : ""}" data-segment="${segment}" d="${segmentPath(offset, lit)}"/>`;
    })
    .join("");
  const marks = offsets
    .filter((offset) => Math.abs(offset) === 1)
    .map((offset) => {
      const p = middle(offset);
      return `<div class="ring-mark off1" style="left:${p.x.toFixed(1)}px;top:${p.y.toFixed(1)}px">${mark(ring.at(offset), 30, state.paused)}</div>`;
    })
    .join("");
  const top = middle(0);
  root.innerHTML =
    `<svg class="ring-arc" viewBox="0 0 ${WIDTH} ${HEIGHT}" width="${WIDTH}" height="${HEIGHT}" aria-hidden="true">${segs}</svg>` +
    marks +
    `<div class="ring-lit" style="left:${top.x.toFixed(1)}px;top:${top.y.toFixed(1)}px" role="status" aria-live="polite">` +
    `${mark(ring.segment(), 34, state.paused)}<span class="ring-name">${esc(state.names[ring.lit] ?? "")}</span><span class="ring-line">${esc(state.lines[ring.lit] ?? "")}</span></div>`;
}

export function mountRing(root: HTMLElement, state: RingState, act: RingActions, now: () => number = () => performance.now()): RingView {
  const ring = new Ring(state.items, state.lit);
  const opened = now();
  // Let go of the modifier soon after opening: a tap, and the ring stays for keys and the mouse.
  let tapped = false;
  let picked = false;

  const pick = (index: number) => {
    if (picked) return;
    picked = true;
    act.pick(index);
  };

  const draw = () => drawRing(root, ring, state);

  const turn = (steps: number) => {
    if (!steps) return;
    ring.turn(steps);
    draw();
  };

  document.addEventListener("keydown", (e) => {
    const k = e.key;
    if (k === "ArrowRight" || k === "ArrowDown") turn(1);
    else if (k === "ArrowLeft" || k === "ArrowUp") turn(-1);
    else if (k === "Enter") pick(ring.lit);
    else if (k === "Escape") act.close();
    // 1 to 9 open the first nine directly.
    else if (/^[1-9]$/.test(k) && Number(k) <= ring.items.length) pick(Number(k) - 1);
    // The shortcut's own key with Shift turns back; without, Kairo turns it (the key is taken
    // by the system before the page sees it).
    else if (k.length === 1 && e.altKey && e.shiftKey) turn(-1);
    else return;
    e.preventDefault();
  });
  document.addEventListener("keyup", (e) => {
    if (e.key !== "Alt" || tapped || picked) return;
    if (onModifierUp(now() - opened) === "stay") tapped = true;
    else pick(ring.lit);
  });

  root.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      turn(e.deltaY > 0 ? 1 : e.deltaY < 0 ? -1 : 0);
    },
    { passive: false },
  );

  // Dragging round the ring turns it with the pointer and leaves it there: a click or Enter then
  // opens the lit one. A press without a drag opens what's under it.
  let press: { x: number; y: number; angle: number; turned: number } | null = null;
  const local = (e: PointerEvent) => {
    const box = root.getBoundingClientRect();
    return { x: e.clientX - box.left, y: e.clientY - box.top };
  };
  root.addEventListener("pointerdown", (e) => {
    const p = local(e);
    press = { ...p, angle: angle(p.x, p.y), turned: 0 };
    root.setPointerCapture?.(e.pointerId);
  });
  root.addEventListener("pointermove", (e) => {
    if (!press) return;
    const p = local(e);
    if (Math.hypot(p.x - press.x, p.y - press.y) < DRAG_PX && !press.turned) return;
    const steps = dragSteps(press.angle, angle(p.x, p.y));
    turn(steps - press.turned);
    press.turned = steps;
    root.classList.add("dragging");
  });
  // A drag cut off before the button came up (the window lost the pointer, a touch was
  // cancelled) ends where it is, so moving the mouse afterwards doesn't keep turning the ring.
  const drop = () => {
    press = null;
    root.classList.remove("dragging");
  };
  root.addEventListener("pointercancel", drop);
  root.addEventListener("lostpointercapture", drop);
  window.addEventListener("blur", drop);
  root.addEventListener("pointerup", (e) => {
    if (!press) return;
    const p = local(e);
    const dragged = root.classList.contains("dragging");
    root.classList.remove("dragging");
    press = null;
    if (dragged) {
      // The ring now waits for a click or Enter, so letting go of a held modifier doesn't open it.
      tapped = true;
      return;
    }
    const under = ring.hit(p.x, p.y);
    if (under !== null) pick(under);
  });

  draw();
  return { turn, lit: () => ring.lit };
}
