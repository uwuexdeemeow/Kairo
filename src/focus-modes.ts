import { api, type FocusMode, type InstalledApp } from "./api";
import { confirmTwice } from "./confirm";
import { $, fmtDur, h, why } from "./ui";

// Chrono's Focus page, Modes: each a name for a way of working, with how long to focus, the block
// list that applies and the apps to open. Typed in the palette by its name, a mode starts the
// session, applies the list and opens the apps.

/// The lengths a mode offers, in minutes (one made some other way keeps its own).
const LENGTHS = [15, 25, 30, 45, 50, 60, 90, 120, 180, 240];

/// The installed apps, read the first time one is added to a mode: listing them is work.
let installed: InstalledApp[] | null = null;

/// Forgets the installed apps read, so the next one added reads them again.
export function forgetInstalledApps(): void {
  installed = null;
}

/// Draws the modes, `lists` the saved block lists' names to choose from. A change saves, then
/// redraws what was kept; a failure says why in the line beside the Add button.
export function renderModes(modes: FocusMode[], lists: string[]): void {
  const status = $("#mode-status");
  const run = async (change: () => Promise<FocusMode[]>) => {
    try {
      renderModes(await change(), lists);
      status.textContent = "";
    } catch (err) {
      status.textContent = why(err);
    }
  };
  $("#modes").replaceChildren(
    ...modes.map((m) => {
      const save = (next: Partial<FocusMode> & { name?: string }) => run(() => api.saveFocusMode(m.name, next.name ?? m.name, next.minutes ?? m.minutes, next.list === undefined ? m.list : next.list, next.apps ?? m.apps));

      const name = h("input", { class: "filter mode-name", value: m.name, maxlength: "30", "aria-label": "Mode name" }) as HTMLInputElement;
      name.addEventListener("change", () => save({ name: name.value }));

      const lengths = LENGTHS.includes(m.minutes) ? LENGTHS : [...LENGTHS, m.minutes].sort((a, b) => a - b);
      const length = h("select", { class: "mode-length", "aria-label": "Focus for" }, ...lengths.map((n) => h("option", { value: String(n) }, fmtDur(n * 60_000)))) as HTMLSelectElement;
      length.value = String(m.minutes);
      length.addEventListener("change", () => save({ minutes: Number(length.value) }));

      const list = h("select", { class: "mode-list", "aria-label": "Block list" }, h("option", { value: "" }, "What a session blocks"), ...lists.map((l) => h("option", { value: l }, `Your “${l}” list`))) as HTMLSelectElement;
      list.value = m.list ?? "";
      list.addEventListener("change", () => save({ list: list.value || null }));

      const remove = h("button", { class: "link-btn" }, "Remove") as HTMLButtonElement;
      confirmTwice(remove, () => "Click again to remove", () => run(() => api.removeFocusMode(m.name)));

      // The apps it opens, each with its way out, and a way to add another: the installed apps are
      // read when that is asked for.
      const apps = h("div", { class: "mode-apps", role: "group", "aria-label": `Apps ${m.name} opens` });
      for (const app of m.apps) {
        const out = h("button", { class: "link-btn mode-app-remove", "aria-label": `Don't open ${app.name}`, title: `Don't open ${app.name}` }, "✕") as HTMLButtonElement;
        out.addEventListener("click", () => save({ apps: m.apps.filter((a) => a.path !== app.path) }));
        apps.append(h("span", { class: "mode-app" }, app.name, out));
      }
      const add = h("button", { class: "link-btn mode-app-add" }, m.apps.length ? "Add another app" : "Add an app") as HTMLButtonElement;
      add.addEventListener("click", async () => {
        try {
          installed ??= await api.installedApps();
        } catch (err) {
          status.textContent = why(err);
          return;
        }
        const taken = new Set(m.apps.map((a) => a.path));
        const pick = h("select", { class: "mode-app-pick", "aria-label": "App to open" }, h("option", { value: "" }, "Choose an app…"), ...installed.filter((a) => !taken.has(a.path)).map((a) => h("option", { value: a.path }, a.name))) as HTMLSelectElement;
        pick.addEventListener("change", () => {
          const chosen = installed?.find((a) => a.path === pick.value);
          if (chosen) save({ apps: [...m.apps, chosen] });
        });
        add.replaceWith(pick);
        pick.focus();
      });
      apps.append(add);

      return h("div", { class: "block-list mode" }, h("div", { class: "row-buttons" }, name, length, list, remove), apps);
    }),
  );
}

/// The add row: a name, and a mode of 25 minutes that blocks what a session does, to change from there.
export function setUpModeForm(lists: () => string[]): void {
  $("#mode-add").addEventListener("click", async () => {
    const name = $<HTMLInputElement>("#mode-name");
    try {
      renderModes(await api.saveFocusMode(null, name.value, 25, null, []), lists());
      name.value = "";
      $("#mode-status").textContent = "";
    } catch (err) {
      $("#mode-status").textContent = why(err);
    }
  });
}
