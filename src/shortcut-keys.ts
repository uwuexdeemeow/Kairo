import { IS_LINUX, IS_MAC, h } from "./ui";

/// A key as this computer's keyboard labels it.
export function keyName(key: string): string {
  if (IS_MAC) return { Ctrl: "Control", Alt: "Option", Super: "Command" }[key] ?? key;
  if (!IS_LINUX && key === "Super") return "Win";
  return key;
}

/// A saved shortcut ("Ctrl+Space") as keys, named the way this computer's keyboard labels them.
export function shortcutKeys(shortcut: string): HTMLElement[] {
  return shortcut.split("+").map((k) => h("kbd", {}, keyName(k)));
}
