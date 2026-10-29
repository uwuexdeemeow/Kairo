import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { shownControl, useDropdowns } from "./dropdown";

// The page's MutationObservers and setter hooks answer after the current task.
const settle = () => new Promise((r) => setTimeout(r, 0));

let select: HTMLSelectElement;
let button: HTMLButtonElement;
const changes = vi.fn();

const list = () => document.querySelector<HTMLElement>(".dropdown-list");
const shownLabel = () => button.querySelector(".dropdown-labels .current")?.textContent;
const press = (key: string, init: KeyboardEventInit = {}) => {
  const e = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  button.dispatchEvent(e);
  return e;
};

beforeAll(() => useDropdowns());

// What's typed is forgotten after a moment; each test starts well past the last one's.
let now = Date.now();

beforeEach(async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime((now += 10_000));
  document.body.innerHTML = `
    <label>Show
      <select id="pick" aria-label="Whose time">
        <option value="all">All computers</option>
        <option value="this">This one (MacBook)</option>
        <option value="tower" disabled>Tower</option>
        <option value="thinkpad">ThinkPad</option>
      </select>
    </label>`;
  await settle();
  select = document.querySelector("select")!;
  button = document.querySelector(".dropdown-button")!;
  changes.mockReset();
  select.addEventListener("change", changes);
});

afterEach(() => vi.useRealTimers());

describe("a themed dropdown", () => {
  it("hides the select behind a button that shows the choice", () => {
    expect(button).not.toBeNull();
    expect(select.classList.contains("dropdown-native")).toBe(true);
    expect(select.tabIndex).toBe(-1);
    expect(shownLabel()).toBe("All computers");
    expect(button.getAttribute("aria-label")).toBe("Whose time");
  });

  it("follows the select's value set from code", async () => {
    select.value = "thinkpad";
    await settle();
    expect(shownLabel()).toBe("ThinkPad");
  });

  it("follows options rebuilt from code", async () => {
    select.replaceChildren(new Option("Dark", "dark"), new Option("Light", "light"));
    await settle();
    expect(shownLabel()).toBe("Dark");
  });

  it("opens a list on click and chooses with the pointer", () => {
    button.click();
    expect(list()).not.toBeNull();
    expect(button.getAttribute("aria-expanded")).toBe("true");
    const options = list()!.querySelectorAll<HTMLElement>("[role=option]");
    expect(options).toHaveLength(4);
    expect(options[2].getAttribute("aria-disabled")).toBe("true");

    options[3].dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, cancelable: true }));
    expect(select.value).toBe("thinkpad");
    expect(changes).toHaveBeenCalledOnce();
    expect(list()).toBeNull();
    expect(button.getAttribute("aria-expanded")).toBe("false");
  });

  it("closes on a click elsewhere without choosing", () => {
    button.click();
    document.body.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    expect(list()).toBeNull();
    expect(changes).not.toHaveBeenCalled();
  });

  it("gives disabled selects a disabled button that doesn't open", async () => {
    select.disabled = true;
    await settle();
    expect(button.disabled).toBe(true);
  });
});

describe("its keys", () => {
  it("change the choice with the arrows while closed, skipping disabled options", () => {
    press("ArrowDown");
    expect(select.value).toBe("this");
    press("ArrowDown");
    expect(select.value).toBe("thinkpad");
    expect(changes).toHaveBeenCalledTimes(2);
    press("ArrowUp");
    expect(select.value).toBe("this");
  });

  it("go to the first and last options with Home and End", () => {
    press("End");
    expect(select.value).toBe("thinkpad");
    press("Home");
    expect(select.value).toBe("all");
  });

  it("move through the open list and choose with Enter", () => {
    press("Enter");
    expect(list()).not.toBeNull();
    press("ArrowDown");
    expect(select.value).toBe("all");
    expect(list()!.querySelector(".active")?.textContent).toBe("This one (MacBook)");
    press("Enter");
    expect(select.value).toBe("this");
    expect(list()).toBeNull();
  });

  it("open the list with Space", () => {
    expect(press(" ").defaultPrevented).toBe(true);
    expect(list()).not.toBeNull();
  });

  it("close the list with Escape, keeping the choice", () => {
    press("Enter");
    press("ArrowDown");
    press("Escape");
    expect(list()).toBeNull();
    expect(select.value).toBe("all");
  });

  it("jump to the option that starts with what's typed", () => {
    press("t");
    expect(select.value).toBe("this");
    press("h");
    press("i");
    press("n");
    expect(select.value).toBe("thinkpad");
  });

  it("take a space typed mid type-ahead as part of it, not as opening the list", () => {
    press("t");
    press("h");
    press("i");
    press("s");
    const space = press(" ");
    expect(space.defaultPrevented).toBe(true);
    expect(list()).toBeNull();
    press("o");
    expect(select.value).toBe("this");
  });

  it("start type-ahead over after a pause", () => {
    press("t");
    vi.setSystemTime(now + 1000);
    press("a");
    expect(select.value).toBe("all");
  });
});

describe("shownControl", () => {
  it("gives a select's button, since the select is hidden", () => {
    expect(shownControl(select)).toBe(button);
  });

  it("gives anything else as it is", () => {
    const input = document.createElement("input");
    expect(shownControl(input)).toBe(input);
    expect(shownControl(document.createElement("select"))).toBeInstanceOf(HTMLSelectElement);
  });
});
