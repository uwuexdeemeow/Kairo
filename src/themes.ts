// Kairo's themes: a ground, or matching the system. Each ground is a paper over the dark or the
// light set (`data-theme`), named on the page by `data-ground`; Ink and Paper are the sets
// themselves, saved as "dark" and "light" from before there were others. The same list, with its
// colors, is in src-tauri/src/appearance.rs.

export type Ground = { id: string; name: string; light: boolean };

export const GROUNDS: Ground[] = [
  { id: "dark", name: "Ink", light: false },
  { id: "graphite", name: "Graphite", light: false },
  { id: "walnut", name: "Walnut", light: false },
  { id: "baize", name: "Baize", light: false },
  { id: "midnight", name: "Midnight", light: false },
  { id: "light", name: "Paper", light: true },
  { id: "linen", name: "Linen", light: true },
  { id: "sage", name: "Sage", light: true },
];

/// What a page wears for a choice: the set, and the ground over it when it isn't Ink or Paper.
/// "system" (or a choice from a newer Kairo) takes Ink or Paper as the system looks.
export function look(mode: string, systemLight: boolean): { theme: "light" | "dark"; ground: string | null } {
  const ground = GROUNDS.find((g) => g.id === mode);
  if (!ground) return { theme: systemLight ? "light" : "dark", ground: null };
  return { theme: ground.light ? "light" : "dark", ground: ground.id === "dark" || ground.id === "light" ? null : ground.id };
}

/// Puts a look on the page.
export function wear(root: HTMLElement, { theme, ground }: ReturnType<typeof look>) {
  root.dataset.theme = theme;
  if (ground) root.dataset.ground = ground;
  else delete root.dataset.ground;
}
