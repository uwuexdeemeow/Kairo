import { beforeEach, describe, expect, it, vi } from "vitest";
import { continueList, highlight, mountEditor, toggleBoxAt, wrapSelection } from "./note-editor";

const mark = (m: string) => `<span class="md-mark">${m}</span>`;

describe("highlight", () => {
  it("styles headings, keeping their marks", () => {
    expect(highlight("# Plan")).toBe(`<span class="md-h">${mark("# ")}Plan</span>`);
    expect(highlight("### Deep")).toBe(`<span class="md-h">${mark("### ")}Deep</span>`);
    // Not a heading without the space, or past three.
    expect(highlight("#tag")).toBe("#tag");
    expect(highlight("#### four")).toBe("#### four");
  });

  it("styles bold, italic, code and links", () => {
    expect(highlight("a **b** c")).toBe(`a <strong class="md-b">${mark("**")}b${mark("**")}</strong> c`);
    expect(highlight("*i* _j_")).toBe(`<em class="md-i">${mark("*")}i${mark("*")}</em> <em class="md-i">${mark("_")}j${mark("_")}</em>`);
    expect(highlight("run `npm i`")).toBe(`run <code class="md-c">${mark("`")}npm i${mark("`")}</code>`);
    expect(highlight("[docs](https://x.y)")).toBe(`<span class="md-link">${mark("[")}docs${mark("](https://x.y)")}</span>`);
  });

  it("draws checkboxes, done ones and their text faded", () => {
    expect(highlight("- [ ] milk")).toBe(`${mark("- ")}<span class="md-box">[ ]</span> milk`);
    expect(highlight("- [x] eggs")).toBe(`${mark("- ")}<span class="md-box md-done">[x]</span><span class="md-done-text"> eggs</span>`);
  });

  it("escapes the text so a note can't add markup", () => {
    expect(highlight("<b>&</b>")).toBe("&lt;b&gt;&amp;&lt;/b&gt;");
    expect(highlight("**<i>**")).toBe(`<strong class="md-b">${mark("**")}&lt;i&gt;${mark("**")}</strong>`);
  });

  it("keeps every line, and room after a last empty one", () => {
    expect(highlight("a\n\nb\n")).toBe("a\n\nb\n ");
  });
});

describe("lists", () => {
  it("continues a list on Enter", () => {
    expect(continueList("- milk", 6)).toEqual({ text: "- milk\n- ", caret: 9 });
    expect(continueList("  * a", 5)).toEqual({ text: "  * a\n  * ", caret: 10 });
    expect(continueList("1. one", 6)).toEqual({ text: "1. one\n2. ", caret: 10 });
    // A ticked box continues unticked.
    expect(continueList("- [x] done", 10)).toEqual({ text: "- [x] done\n- [ ] ", caret: 17 });
  });

  it("ends the list on an empty item, and leaves other lines alone", () => {
    expect(continueList("- a\n- ", 6)).toEqual({ text: "- a\n", caret: 4 });
    expect(continueList("- [ ] ", 6)).toEqual({ text: "", caret: 0 });
    expect(continueList("plain", 5)).toBeNull();
  });

  it("splits an item when Enter comes mid-line", () => {
    expect(continueList("- milk eggs", 7)).toEqual({ text: "- milk \n- eggs", caret: 10 });
  });

  it("ticks a box only from inside its brackets", () => {
    expect(toggleBoxAt("- [ ] milk", 3)).toBe("- [x] milk");
    expect(toggleBoxAt("a\n- [x] eggs", 5)).toBe("a\n- [ ] eggs");
    expect(toggleBoxAt("- [ ] milk", 8)).toBeNull();
    expect(toggleBoxAt("milk [ ]", 6)).toBeNull();
  });

  it("wraps the selection for bold, or leaves the caret between marks", () => {
    expect(wrapSelection("a b c", 2, 3, "**")).toEqual({ text: "a **b** c", start: 4, end: 5 });
    expect(wrapSelection("ab", 1, 1, "*")).toEqual({ text: "a**b", start: 2, end: 2 });
  });
});

describe("the editor", () => {
  let root: HTMLElement;
  beforeEach(() => {
    document.body.innerHTML = `<div id="root"></div>`;
    root = document.getElementById("root")!;
  });

  it("draws the text behind the textarea and reports typing", () => {
    const changed = vi.fn();
    const ed = mountEditor(root, "# Plan", changed);
    const area = root.querySelector("textarea")!;
    expect(area.value).toBe("# Plan");
    expect(area.maxLength).toBe(100_000);
    expect(root.querySelector(".note-layer")!.innerHTML).toBe(highlight("# Plan"));
    area.value = "# Plan\n**x**";
    area.dispatchEvent(new Event("input"));
    expect(changed).toHaveBeenCalledWith("# Plan\n**x**");
    expect(root.querySelector(".note-layer")!.innerHTML).toBe(highlight("# Plan\n**x**"));
    ed.set("other");
    expect(ed.value()).toBe("other");
    // Setting it from outside isn't typing.
    expect(changed).toHaveBeenCalledTimes(1);
  });

  it("shows a prompt only when it's given one", () => {
    mountEditor(root, "", () => {}, "# A title");
    expect(root.querySelector("textarea")!.placeholder).toBe("# A title");
    mountEditor(root, "", () => {});
    expect(root.querySelector("textarea")!.placeholder).toBe("");
  });

  it("keeps the caret where it was when the text is set from outside", () => {
    const ed = mountEditor(root, "- [ ] milk\n- [ ] eggs", () => {});
    const area = root.querySelector("textarea")!;
    area.setSelectionRange(4, 4);
    ed.set("- [x] milk\n- [ ] eggs");
    expect([area.selectionStart, area.selectionEnd]).toEqual([4, 4]);
    // Past the end of a shorter text, it goes to the end.
    area.setSelectionRange(20, 20);
    ed.set("short");
    expect([area.selectionStart, area.selectionEnd]).toEqual([5, 5]);
  });

  it("continues lists on Enter and ticks boxes on click", () => {
    const changed = vi.fn();
    mountEditor(root, "- [ ] milk", changed);
    const area = root.querySelector("textarea")!;
    area.setSelectionRange(10, 10);
    area.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", cancelable: true }));
    expect(area.value).toBe("- [ ] milk\n- [ ] ");
    area.setSelectionRange(3, 3);
    area.dispatchEvent(new MouseEvent("click"));
    expect(area.value).toBe("- [x] milk\n- [ ] ");
    expect(changed).toHaveBeenLastCalledWith("- [x] milk\n- [ ] ");
  });

  it("makes the selection bold with Ctrl+B", () => {
    mountEditor(root, "a b", () => {});
    const area = root.querySelector("textarea")!;
    area.setSelectionRange(2, 3);
    area.dispatchEvent(new KeyboardEvent("keydown", { key: "b", ctrlKey: true, cancelable: true }));
    expect(area.value).toBe("a **b**");
  });
});
