import { api } from "./api";
import { logo, type AppName } from "./logos";
import { shortcutKeys } from "./shortcut-keys";
import { $, IS_MAC, h } from "./ui";

// The first time Kairo opens, one full-page walkthrough of four short screens: what it is, the
// palette, Chrono, and Ledger with focus. It covers Chrono's window until it's finished or skipped,
// is remembered as seen, and can be shown again from Chrono's settings.

type Step = { app: AppName; title: string; body: () => Promise<Node[]> };

/// The palette's keys as this computer has them set; the usual ones when they can't be read.
async function paletteKeys(): Promise<HTMLElement> {
  let shortcut = IS_MAC ? "Alt+Space" : "Ctrl+Space";
  try {
    shortcut = (await api.paletteSettings()).shortcut || shortcut;
  } catch {
    // The keys the palette starts with.
  }
  return h("span", { class: "shortcut-keys" }, ...shortcutKeys(shortcut));
}

const STEPS: Step[] = [
  {
    app: "kairo",
    title: "Welcome to Kairo",
    body: async () => [
      h("p", {}, "A small hub for your desktop: a command palette, a screen time tracker and your tasks."),
      h("p", { class: "muted" }, "Everything stays on this computer."),
    ],
  },
  {
    app: "palette",
    title: "The command palette",
    body: async () => {
      const try_ = h("button", { class: "btn", id: "walkthrough-try", type: "button" }, "Try it") as HTMLButtonElement;
      const note = h("span", { class: "muted", id: "walkthrough-note", "aria-live": "polite" });
      try_.addEventListener("click", async () => {
        try {
          await api.openPalette();
          note.textContent = "";
        } catch {
          note.textContent = "Couldn't open the palette.";
        }
      });
      return [
        h("p", {}, "Press ", await paletteKeys(), " anywhere. Open an app or a file, work something out, start a focus session or add a task."),
        h("p", {}, "Type ", h("kbd", {}, "?"), " in it to see everything it can do."),
        h("div", { class: "row-buttons" }, try_, note),
      ];
    },
  },
  {
    app: "chrono",
    title: "Chrono keeps your time",
    body: async () => [
      h("p", {}, "It is already noting which app is in front, once a second. Give it an hour and Today fills in."),
      h("p", { class: "muted" }, "A daily goal and the browser extension can wait. Both are in Settings."),
    ],
  },
  {
    app: "ledger",
    title: "Ledger and focus",
    body: async () => [
      h("p", {}, "Ledger holds your tasks and notes. A focus session blocks distracting apps for a stretch you choose."),
      h("p", {}, "From the palette, type ", h("kbd", {}, "focus 25"), " to start one."),
    ],
  },
];

/// Sets the walkthrough up once, and shows it when this is the first run. `openSettings` is what
/// the last screen's link does.
export function setUpWalkthrough(openSettings: () => void): void {
  const root = $("#walkthrough");
  const stepBox = $("#walkthrough-step");
  const dots = $("#walkthrough-dots");
  const [skip, back, next, more] = ["#walkthrough-skip", "#walkthrough-back", "#walkthrough-next", "#walkthrough-settings"].map((s) => $<HTMLButtonElement>(s));
  let at = 0;
  // Everything behind it, which can't be reached while it's open.
  const behind = () => [...document.body.children].filter((e) => e !== root && e.id !== "tooltip" && e.tagName !== "SCRIPT") as HTMLElement[];

  const draw = async () => {
    const step = STEPS[at];
    root.dataset.app = step.app;
    const last = at === STEPS.length - 1;
    const body = await step.body();
    // A quick click through the screens can leave an earlier one's drawing finishing late.
    if (STEPS[at] !== step) return;
    const title = h("h1", { class: "walkthrough-title", id: "walkthrough-title", tabindex: "-1" }, step.title);
    stepBox.replaceChildren(h("span", { class: "walkthrough-mark" }), title, ...body);
    stepBox.querySelector(".walkthrough-mark")!.innerHTML = logo(step.app, 44);
    dots.replaceChildren(...STEPS.map((_, i) => h("span", { class: i === at ? "dot on" : "dot" })));
    dots.setAttribute("aria-label", `Step ${at + 1} of ${STEPS.length}`);
    back.hidden = at === 0;
    skip.hidden = last;
    more.hidden = !last;
    next.textContent = last ? "Start" : "Next";
    title.focus();
  };

  const close = async (then?: () => void) => {
    root.hidden = true;
    for (const e of behind()) e.inert = false;
    document.removeEventListener("keydown", keys, true);
    try {
      await api.finishWalkthrough();
    } catch {
      // It will be offered again next time, which is better than losing a click.
    }
    then?.();
  };

  const go = (to: number) => {
    at = Math.min(Math.max(to, 0), STEPS.length - 1);
    void draw();
  };

  function keys(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      void close();
    } else if (e.key === "ArrowRight" && at < STEPS.length - 1) {
      go(at + 1);
    } else if (e.key === "ArrowLeft" && at > 0) {
      go(at - 1);
    }
  }

  const show = () => {
    at = 0;
    root.hidden = false;
    for (const e of behind()) e.inert = true;
    document.addEventListener("keydown", keys, true);
    void draw();
  };

  skip.addEventListener("click", () => void close());
  back.addEventListener("click", () => go(at - 1));
  next.addEventListener("click", () => (at === STEPS.length - 1 ? void close() : go(at + 1)));
  more.addEventListener("click", () => void close(openSettings));
  $("#walkthrough-again").addEventListener("click", show);

  api.walkthroughDue().then((due) => due && show(), () => {});
}
