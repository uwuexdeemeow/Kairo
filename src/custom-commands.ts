import { api, type CustomCommand, type CustomKind } from "./api";
import { confirmTwice } from "./confirm";
import { $, h, why } from "./ui";

// Kairo's settings, Your commands: each a name you type in the palette that opens a link, opens a
// file, folder or app, or copies some text.

/// What each kind says it does, and what its box asks for.
export const KINDS: { kind: CustomKind; label: string; asks: string }[] = [
  { kind: "link", label: "Opens a link", asks: "Where it goes, like github.com/you/repo/issues" },
  { kind: "open", label: "Opens a file, folder or app", asks: "Its path, like C:\\Users\\you\\Documents\\Tax" },
  { kind: "text", label: "Copies text", asks: "The text to copy" },
];

const kindSelect = (cls: string, label: string): HTMLSelectElement =>
  h("select", { class: cls, "aria-label": label }, ...KINDS.map((k) => h("option", { value: k.kind }, k.label))) as HTMLSelectElement;

/// Draws the commands. A change saves, then redraws what was kept; a failure says why in the line
/// beside the Add button.
export function renderCustomCommands(commands: CustomCommand[]): void {
  const status = $("#custom-status");
  const run = async (change: () => Promise<CustomCommand[]>) => {
    try {
      renderCustomCommands(await change());
      status.textContent = "";
    } catch (err) {
      status.textContent = why(err);
    }
  };
  $("#custom-list").replaceChildren(
    ...commands.map((c) => {
      const save = (next: Partial<CustomCommand>) => run(() => api.saveCustomCommand(c.name, next.name ?? c.name, next.kind ?? c.kind, next.target ?? c.target));
      const name = h("input", { class: "filter custom-name", value: c.name, maxlength: "30", "aria-label": "Command name", autocomplete: "off" }) as HTMLInputElement;
      name.addEventListener("change", () => save({ name: name.value }));
      const kind = kindSelect("custom-kind", `What ${c.name} does`);
      kind.value = c.kind;
      kind.addEventListener("change", () => save({ kind: kind.value as CustomKind }));
      const target = h("textarea", { class: "filter custom-target", rows: "2", "aria-label": `${c.name}: ${KINDS.find((k) => k.kind === c.kind)?.asks ?? ""}`, spellcheck: "false" }, c.target) as HTMLTextAreaElement;
      target.addEventListener("change", () => save({ target: target.value }));
      const remove = h("button", { class: "link-btn", type: "button" }, "Remove") as HTMLButtonElement;
      confirmTwice(remove, () => "Press again to remove", () => run(() => api.removeCustomCommand(c.name)));
      return h("div", { class: "block-list custom-command" }, h("div", { class: "row-buttons" }, name, kind, remove), target);
    }),
  );
}

/// The add row: a name, what it does and where. The box says what it wants for the kind chosen.
export function setUpCustomForm(): void {
  const kind = $<HTMLSelectElement>("#custom-new-kind");
  const target = $<HTMLTextAreaElement>("#custom-new-target");
  const ask = () => {
    const asks = KINDS.find((k) => k.kind === kind.value)?.asks ?? "";
    target.placeholder = asks;
    target.setAttribute("aria-label", asks);
  };
  kind.replaceChildren(...KINDS.map((k) => h("option", { value: k.kind }, k.label)));
  kind.addEventListener("change", ask);
  ask();
  $("#custom-add").addEventListener("click", async () => {
    const name = $<HTMLInputElement>("#custom-new-name");
    try {
      renderCustomCommands(await api.saveCustomCommand(null, name.value, kind.value as CustomKind, target.value));
      name.value = "";
      target.value = "";
      $("#custom-status").textContent = "";
      name.focus();
    } catch (err) {
      $("#custom-status").textContent = why(err);
    }
  });
}

/// Reads the commands when settings open.
export async function loadCustomCommands(): Promise<void> {
  renderCustomCommands(await api.customCommands());
}
