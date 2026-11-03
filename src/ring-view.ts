/// The web ring's page (macOS, Linux): the arc of segments, the lit one at the top with its logo,
/// name and line, and everything that turns it or picks from it.
import { logo } from "./logos";
import { CENTRE, DRAG_PX, HEIGHT, INNER, OUTER, Ring, SEGMENTS, STEP, WIDTH, angle, dragSteps, onModifierUp, segmentPath, type Segment } from "./ring-model";

export type RingState = { lit: number; lines: string[]; focusRunning: boolean };
export type RingActions = { pick(index: number): void; close(): void };
export type RingView = { turn(steps: number): void; lit(): number };

const NAMES: Record<Segment, string> = {
  chrono: "Chrono",
  ledger: "Ledger",
  palette: "Palette",
  dashboard: "Dashboard",
  task: "New task",
  note: "New note",
  focus: "Focus 25",
};

/// The quick actions' marks, drawn like the logos.
const MARKS: Record<"task" | "note" | "focus", string> = {
  task: `<path d="M50 22v56M22 50h56" style="stroke:var(--ledger)" stroke-width="10" stroke-linecap="round"/>`,
  note: `<path d="M28 74l6-20 34-34 14 14-34 34z" style="fill:none;stroke:var(--ledger)" stroke-width="7" stroke-linejoin="round"/><path d="M28 80h44" style="stroke:var(--ink)" stroke-width="5" stroke-linecap="round" opacity=".6"/>`,
  focus: `<circle cx="50" cy="54" r="32" style="fill:none;stroke:var(--chrono)" stroke-width="7"/><path d="M50 54V36M42 14h16" style="stroke:var(--ink)" stroke-width="7" stroke-linecap="round"/>`,
};

function mark(segment: Segment, size: number): string {
  switch (segment) {
    case "chrono":
    case "ledger":
    case "palette":
      return logo(segment, size);
    case "dashboard":
      return logo("kairo", size);
    default:
      return `<svg class="logo" width="${size}" height="${size}" viewBox="0 0 100 100" aria-hidden="true">${MARKS[segment]}</svg>`;
  }
}

const esc = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

/// Where a segment's middle is, `offset` places from the top.
function middle(offset: number): { x: number; y: number } {
  const r = (OUTER + INNER) / 2 + (offset === 0 ? 8 : 0);
  const a = (offset * STEP * Math.PI) / 180;
  return { x: CENTRE.x + r * Math.sin(a), y: CENTRE.y - r * Math.cos(a) };
}

export function mountRing(root: HTMLElement, state: RingState, act: RingActions, now: () => number = () => performance.now()): RingView {
  const ring = new Ring(state.lit);
  const opened = now();
  // Let go of the modifier soon after opening: a tap, and the ring stays for keys and the mouse.
  let tapped = false;
  let picked = false;

  const pick = (index: number) => {
    if (picked) return;
    picked = true;
    act.pick(index);
  };

  const draw = () => {
    const segs = [-2, -1, 0, 1, 2]
      .map((offset) => {
        const segment = ring.at(offset);
        const lit = offset === 0;
        const dim = segment === "focus" && state.focusRunning;
        return `<path class="ring-seg seg-${segment} off${Math.abs(offset)}${lit ? " lit" : ""}${dim ? " dim" : ""}" data-segment="${segment}" d="${segmentPath(offset, lit)}"/>`;
      })
      .join("");
    const marks = [-1, 1]
      .map((offset) => {
        const p = middle(offset);
        return `<div class="ring-mark off1" style="left:${p.x.toFixed(1)}px;top:${p.y.toFixed(1)}px">${mark(ring.at(offset), 30)}</div>`;
      })
      .join("");
    const top = middle(0);
    const segment = ring.segment();
    root.innerHTML =
      `<svg class="ring-arc" viewBox="0 0 ${WIDTH} ${HEIGHT}" width="${WIDTH}" height="${HEIGHT}" aria-hidden="true">${segs}</svg>` +
      marks +
      `<div class="ring-lit" style="left:${top.x.toFixed(1)}px;top:${top.y.toFixed(1)}px" role="status" aria-live="polite">` +
      `${mark(segment, 34)}<span class="ring-name">${esc(NAMES[segment])}</span><span class="ring-line">${esc(state.lines[ring.lit] ?? "")}</span></div>`;
  };

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
    else if (/^[1-7]$/.test(k)) pick(Number(k) - 1);
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
    if (under) pick(SEGMENTS.indexOf(under));
  });

  draw();
  return { turn, lit: () => ring.lit };
}
