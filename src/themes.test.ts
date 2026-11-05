import { describe, expect, it } from "vitest";
import { GROUNDS, look, wear } from "./themes";
import dashboard from "../dashboard.html?raw";

describe("themes", () => {
  it("keep Ink and Paper as the sets themselves, under their old names", () => {
    expect(look("dark", true)).toEqual({ theme: "dark", ground: null });
    expect(look("light", false)).toEqual({ theme: "light", ground: null });
  });

  it("wear another ground over its own set", () => {
    expect(look("walnut", true)).toEqual({ theme: "dark", ground: "walnut" });
    expect(look("sage", false)).toEqual({ theme: "light", ground: "sage" });
  });

  it("match the system with Ink or Paper, as do choices this page doesn't know", () => {
    expect(look("system", true)).toEqual({ theme: "light", ground: null });
    expect(look("system", false)).toEqual({ theme: "dark", ground: null });
    expect(look("plaid", false)).toEqual({ theme: "dark", ground: null });
  });

  it("take the last ground off when going back to Ink", () => {
    const root = document.createElement("html");
    wear(root, look("baize", false));
    expect(root.dataset.ground).toBe("baize");
    wear(root, look("dark", false));
    expect(root.dataset).toMatchObject({ theme: "dark" });
    expect(root.dataset.ground).toBeUndefined();
  });

  // Their colors are checked against the stylesheet in src-tauri/src/appearance.rs.
  it("are all in the settings' dropdown, under Dark and Light", () => {
    const page = new DOMParser().parseFromString(dashboard, "text/html");
    const groups = [...page.querySelectorAll("#appearance optgroup")].map((g) => [g.getAttribute("label"), [...g.querySelectorAll("option")].map((o) => o.value)]);
    expect(groups).toEqual([
      ["Dark", GROUNDS.filter((g) => !g.light).map((g) => g.id)],
      ["Light", GROUNDS.filter((g) => g.light).map((g) => g.id)],
    ]);
  });
});
