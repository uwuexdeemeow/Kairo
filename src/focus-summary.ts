// Two blocks of the Focus page: how the last session went (its wrap-up), and today against the
// daily focus target. Kairo works both out (src-tauri/src/wrapup.rs, focus.rs); these draw them.

import type { FocusTarget, FocusWrapUp } from "./api";
import { fmtDur, h } from "./ui";

/// Amounts read "0m" rather than "<1m" when there's nothing yet.
const fmtAmount = (ms: number) => (ms < 30_000 ? "0m" : fmtDur(ms));

const nudgesSaid = (n: number) => (n === 0 ? "No nudges" : n === 1 ? "1 nudge" : `${n} nudges`);

/// How the last session went, in `box`, until it's dismissed or another starts; `box` is hidden
/// with nothing to show. `dismiss` puts it away for good.
export function renderWrapUp(box: HTMLElement, w: FocusWrapUp | null, dismiss: () => Promise<void>) {
  box.hidden = !w;
  if (!w) {
    box.replaceChildren();
    return;
  }
  const away = h("button", { class: "link-btn" }, "Dismiss");
  away.addEventListener("click", () => {
    box.hidden = true;
    void dismiss().catch(() => {});
  });
  box.replaceChildren(
    h("div", { class: "section-head" }, h("h2", {}, w.label ? `How “${w.label}” went` : "How your last session went"), away),
    h(
      "p",
      { class: "wrapup-facts" },
      h("span", {}, `Focused ${fmtAmount(w.focusedMs)} of ${fmtDur(w.plannedMs)}`),
      h("span", { class: "muted" }, w.completed ? "Finished" : "Ended early"),
      h("span", {}, nudgesSaid(w.nudges)),
    ),
    ...(w.pulled.length
      ? [
          h("h3", {}, "What pulled you away"),
          h("ul", { class: "wrapup-pulled" }, ...w.pulled.map((p) => h("li", {}, h("span", {}, p.label), h("span", { class: "muted" }, fmtDur(p.ms))))),
        ]
      : [h("p", { class: "muted" }, "Nothing distracting was in front while it ran.")]),
    ...(w.versus.length ? [h("ul", { class: "wrapup-versus muted" }, ...w.versus.map((line) => h("li", {}, line)))] : []),
  );
}

/// Today against the daily focus target, with its streak, in `box`; hidden with no target set.
export function renderFocusTarget(box: HTMLElement, t: FocusTarget | null) {
  box.hidden = !t;
  if (!t) {
    box.replaceChildren();
    return;
  }
  const value = `${fmtAmount(t.usedMs)} of ${fmtDur(t.targetMs)} today`;
  const fill = h("div", { class: "line productive" });
  fill.style.width = `${Math.min(t.usedMs / t.targetMs, 1) * 100}%`;
  box.replaceChildren(
    h("div", { class: "section-head" }, h("h2", {}, "Focus target"), h("span", { class: "muted" }, t.streak ? `${t.streak}-day streak` : "")),
    h(
      "div",
      { class: "usage goal" },
      h("div", { class: "usage-top" }, h("span", { class: "usage-label" }, t.met ? "Reached" : "Focus time"), h("span", { class: "usage-value" }, value)),
      h("div", { class: "track", role: "img", "aria-label": `Focus time: ${value}` }, fill),
    ),
  );
}
