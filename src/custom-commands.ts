import { api, type CustomCommand, type CustomKind } from "./api";
import { confirmTwice } from "./confirm";
import { $, h, why } from "./ui";

// Kairo's settings, Your commands: each a name you type in the palette that opens a link (with
// what's typed after it in the link's {query}), opens a file, folder or app, pastes some text, or
// runs other words in the palette.

/// What each kind says it does, and what its box asks for.
export const KINDS: { kind: CustomKind; label: string; asks: string }[] = [
  { kind: "link", label: "Opens a link", asks: "Where it goes, like github.com/you/repo/issues. Put {query} where what you type after the name goes" },
  { kind: "open", label: "Opens a file, folder or app", asks: "Its path, like C:\\Users\\you\\Documents\\Tax" },
  { kind: "text", label: "Pastes text", asks: "The text to paste. {date}, {time} and {clipboard} are filled in" },
  { kind: "alias", label: "Runs a palette command", asks: "What it stands for, like visual studio code or focus 50 writing" },
];

/// Buttons that fill a path box from the system's dialog, so a path is picked, not typed; `chosen`
/// is given what was picked.
function pickButtons(chosen: (path: string) => void, status: HTMLElement): HTMLElement {
  const pick = (label: string, folder: boolean) => {
    const button = h("button", { class: "link-btn", type: "button" }, label) as HTMLButtonElement;
    button.addEventListener("click", async () => {
      try {
        const path = await api.pickCommandPath(folder);
        if (path) chosen(path);
      } catch (err) {
        status.textContent = why(err);
      }
    });
    return button;
  };
  return h("p", { class: "custom-pick" }, pick("Choose a file or app…", false), pick("Choose a folder…", true));
}

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
      // A path is picked from the dialog and saved at once, like any other change.
      const pick = c.kind === "open" ? [pickButtons((path) => save({ target: path }), status)] : [];
      return h("div", { class: "block-list custom-command" }, h("div", { class: "row-buttons" }, name, kind, remove), target, ...pick);
    }),
  );
}

/// The add row: a name, what it does and where. The box says what it wants for the kind chosen.
export function setUpCustomForm(): void {
  const kind = $<HTMLSelectElement>("#custom-new-kind");
  const target = $<HTMLTextAreaElement>("#custom-new-target");
  const picks = $("#custom-new-pick");
  const ask = () => {
    const asks = KINDS.find((k) => k.kind === kind.value)?.asks ?? "";
    target.placeholder = asks;
    target.setAttribute("aria-label", asks);
    // The dialog is for a path, which is what only one kind asks for.
    picks.hidden = kind.value !== "open";
  };
  const picked = (path: string) => {
    target.value = path;
    target.focus();
  };
  const [file, folder] = [...picks.querySelectorAll<HTMLButtonElement>("button")];
  for (const [button, isFolder] of [[file, false], [folder, true]] as const) {
    button.addEventListener("click", async () => {
      try {
        const path = await api.pickCommandPath(isFolder);
        if (path) picked(path);
      } catch (err) {
        $("#custom-status").textContent = why(err);
      }
    });
  }
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
