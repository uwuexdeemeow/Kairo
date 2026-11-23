import { api, type CustomCommand, type CustomKind, type KeywordsStatus } from "./api";
import { confirmTwice } from "./confirm";
import { $, h, why } from "./ui";

// Kairo's settings, Your commands: each a name you type in the palette that opens a link (with
// what's typed after it in the link's {query}), opens a file, folder or app, pastes some text, or
// runs other words in the palette. Any command can have keys of its own, and a text command a
// keyword typed in any app.

/// A command's keys row as Kairo's settings record shortcuts: where the keys show, the button,
/// the note, and how they're saved. Settings hands over its recorder, so there's one at a time.
export type KeysRow = { keys: string; button: string; note: string; saved: string; save(shortcut: string): Promise<unknown>; does: string };
let record: ((row: KeysRow) => void) | null = null;
/// Keys as this keyboard labels them; plain text until Settings hands over its own.
let keyLabels: (shortcut: string) => HTMLElement[] = (shortcut) => shortcut.split("+").map((k) => h("kbd", {}, k));

/// Settings' shortcut recorder, for each command's Set keys, and how it shows keys.
export function useKeysRecorder(start: (row: KeysRow) => void, labels: (shortcut: string) => HTMLElement[]): void {
  record = start;
  keyLabels = labels;
}

/// Whether keywords work here, as last asked.
let keywordsStatus: KeywordsStatus = "idle";

/// What the keyword box's note says, for how things stand here.
export function keywordNote(status: KeywordsStatus, mac: boolean): string {
  switch (status) {
    case "unavailable":
      return "Keywords don't work on Wayland, which keeps what you type from other apps. Type the name in the palette instead.";
    case "waiting":
      return mac
        ? "Allow Kairo under System Settings, Privacy & Security, Input Monitoring, then quit and reopen Kairo."
        : "Kairo can't hear typing on this computer yet. Restart Kairo to try again.";
    default:
      return "Type it anywhere and it turns into the text. Start it with ; or another mark so it isn't set off by a word.";
  }
}

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
    ...commands.map((c, i) => {
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
      return h("div", { class: "block-list custom-command" }, h("div", { class: "row-buttons" }, name, kind, remove), target, ...pick, keysRow(c, i, run), ...keywordRow(c, run));
    }),
  );
}

/// The command's own keys: shown, with Set keys (Settings' recorder) and Clear.
function keysRow(c: CustomCommand, i: number, run: (change: () => Promise<CustomCommand[]>) => Promise<void>): HTMLElement {
  const row: KeysRow = {
    keys: `#custom-keys-${i}`,
    button: `#custom-record-${i}`,
    note: `#custom-keys-note-${i}`,
    saved: c.keys ?? "",
    // A refusal is thrown back, so the recorder says why under the keys.
    save: async (keys) => renderCustomCommands(await api.setCustomKeys(c.name, keys)),
    does: `runs ${c.name}`,
  };
  const keys = h("span", { class: "shortcut-keys", id: `custom-keys-${i}`, "aria-live": "polite" }, ...(c.keys ? keyLabels(c.keys) : [h("span", { class: "muted" }, "No keys")]));
  const button = h("button", { class: "link-btn custom-record", id: `custom-record-${i}`, type: "button" }, c.keys ? "Change" : "Set keys") as HTMLButtonElement;
  button.addEventListener("click", () => record?.(row));
  const clear = h("button", { class: "link-btn custom-clear-keys", type: "button" }, "Clear") as HTMLButtonElement;
  clear.hidden = !c.keys;
  clear.addEventListener("click", () => run(() => api.setCustomKeys(c.name, "")));
  const note = h("span", { class: "muted", id: `custom-keys-note-${i}`, "aria-live": "polite" });
  return h("div", { class: "row-buttons custom-keys" }, h("span", {}, "Keys"), keys, button, clear, note);
}

/// A text command's keyword box, saved as it changes; nothing for other kinds.
function keywordRow(c: CustomCommand, run: (change: () => Promise<CustomCommand[]>) => Promise<void>): HTMLElement[] {
  if (c.kind !== "text") return [];
  const off = keywordsStatus === "unavailable";
  const box = h("input", {
    class: "filter custom-keyword",
    value: c.keyword ?? "",
    maxlength: "20",
    placeholder: "Keyword, like ;sig",
    "aria-label": `${c.name}: keyword typed anywhere to paste it`,
    autocomplete: "off",
    spellcheck: "false",
  }) as HTMLInputElement;
  box.disabled = off;
  box.addEventListener("change", () => run(() => api.setCustomKeyword(c.name, box.value)));
  const mac = /Mac/.test(navigator.platform);
  return [h("div", { class: "row-buttons custom-keys" }, box), h("p", { class: "muted custom-keyword-note" }, keywordNote(keywordsStatus, mac))];
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
  keywordsStatus = await api.keywordsStatus().catch(() => "idle" as const);
  renderCustomCommands(await api.customCommands());
}
