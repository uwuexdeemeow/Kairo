import { api, type WindowLayout } from "./api";
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

export function wireScripts(): void {
  $("#scripts-open").addEventListener("click", async () => {
    const status = $("#scripts-status");
    try {
      status.textContent = await api.openScriptsFolder();
    } catch (err) {
      status.textContent = why(err);
    }
  });
}
