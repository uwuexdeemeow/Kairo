import { api, type ScriptsFound, type WindowLayout } from "./api";
import { confirmTwice } from "./confirm";
import { $, h, why } from "./ui";

// Kairo's settings, Window layouts and Scripts. Layouts are saved from the palette; here each can
// be given a focus label or removed. Scripts live in a folder, which the button opens.

/// Draws the layouts; a change saves and redraws what was kept, a failure says why below them.
export function renderLayouts(layouts: WindowLayout[]): void {
  const status = $("#layouts-status");
  const run = async (change: () => Promise<WindowLayout[]>) => {
    try {
      renderLayouts(await change());
      status.textContent = "";
    } catch (err) {
      status.textContent = why(err);
    }
  };
  if (!layouts.length) {
    $("#layouts-list").replaceChildren(h("p", { class: "muted empty-note" }, "No layouts yet."));
    return;
  }
  $("#layouts-list").replaceChildren(
    ...layouts.map((l) => {
      const count = l.windows.length;
      const label = h("input", {
        class: "filter layout-label",
        value: l.focusLabel ?? "",
        placeholder: "Focus label, like writing",
        maxlength: "40",
        "aria-label": `Focus label that puts ${l.name} back`,
        autocomplete: "off",
      }) as HTMLInputElement;
      label.addEventListener("change", () => run(() => api.setLayoutFocusLabel(l.name, label.value.trim() || null)));
      const remove = h("button", { class: "link-btn", type: "button" }, "Remove") as HTMLButtonElement;
      confirmTwice(remove, () => "Press again to remove", () => run(() => api.removeLayout(l.name)));
      return h(
        "div",
        { class: "block-list layout-row" },
        h("div", { class: "row-buttons" }, h("strong", {}, l.name), h("span", { class: "muted" }, `${count} ${count === 1 ? "window" : "windows"}`), remove),
        label,
      );
    }),
  );
}

export async function loadLayouts(): Promise<void> {
  renderLayouts(await api.layouts());
}

/// The name box and Save beside the layouts: the open windows, kept under that name.
export function wireLayoutSave(): void {
  const name = $<HTMLInputElement>("#layout-new-name");
  const status = $("#layouts-status");
  const save = async () => {
    try {
      renderLayouts(await api.saveLayout(name.value));
      name.value = "";
      status.textContent = "Saved.";
    } catch (err) {
      status.textContent = why(err);
    }
  };
  $("#layout-save").addEventListener("click", save);
  name.addEventListener("keydown", (e) => e.key === "Enter" && save());
}

const OUTPUTS = { notify: "shows a notification", copy: "copies the result", paste: "pastes the result", silent: "says nothing" } as const;

/// What Kairo found in the scripts folder: each script by title, and the files it left out.
export function renderScripts(found: ScriptsFound): void {
  const rows = found.scripts.map((s) =>
    h(
      "p",
      { class: "script-row" },
      h("strong", {}, s.title),
      h("span", { class: "muted" }, ` · ${s.file}${s.argument ? `, asks for ${s.argument.toLowerCase()}` : ""}, ${OUTPUTS[s.output]}`),
    ),
  );
  if (found.skipped.length) {
    rows.push(h("p", { class: "muted" }, `Not listed, with no @kairo.title line: ${found.skipped.join(", ")}.`));
  }
  if (!rows.length) rows.push(h("p", { class: "muted empty-note" }, "No scripts yet."));
  $("#scripts-list").replaceChildren(...rows);
}

export async function loadScripts(): Promise<void> {
  renderScripts(await api.scripts());
}

/// Save to a file and Add from a file: each says what happened beside the buttons, and an add
/// reads the lists again so what came in shows at once.
export function wireSetupFile(reload: () => Promise<unknown>): void {
  const status = $("#setup-status");
  const buttons = [$<HTMLButtonElement>("#setup-export"), $<HTMLButtonElement>("#setup-import")];
  // The system's dialog takes a while, and a second press mustn't open a second one.
  const run = async (task: () => Promise<string | null>) => {
    for (const b of buttons) b.disabled = true;
    try {
      const said = await task();
      if (said) status.textContent = said;
      else status.textContent = "";
    } catch (err) {
      status.textContent = why(err);
    } finally {
      for (const b of buttons) b.disabled = false;
    }
  };
  buttons[0].addEventListener("click", () => run(async () => {
    const name = await api.exportSetup();
    return name ? `Saved ${name}.` : null;
  }));
  buttons[1].addEventListener("click", () => run(async () => {
    const said = await api.importSetup();
    if (said) await reload().catch(() => {});
    return said;
  }));
}

export function wireScripts(): void {
  const status = $("#scripts-status");
  $("#scripts-open").addEventListener("click", async () => {
    try {
      status.textContent = await api.openScriptsFolder();
      // The folder now exists; the list reads it, and its failing mustn't hide where it is.
      await loadScripts().catch(() => {});
    } catch (err) {
      status.textContent = why(err);
    }
  });
  $("#scripts-refresh").addEventListener("click", async () => {
    try {
      await loadScripts();
      status.textContent = "";
    } catch (err) {
      status.textContent = why(err);
    }
  });
}
