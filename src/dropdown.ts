// Kairo's dropdowns. The list a <select> opens is drawn by the system (Windows' own list, a macOS
// menu, GTK on Linux), which ignores the app's theme; so each <select> gets a themed button and
// list in its place. The <select> stays in the page, hidden, as the source of truth: code keeps
// reading and setting its value, rebuilding its options and listening for "change", and the
// button follows along.

const CHEVRON = '<svg class="dropdown-chevron" viewBox="0 0 24 24" aria-hidden="true"><path d="M6 9l6 6 6-6" /></svg>';
/// Leaves room below the list so it doesn't touch the window's edge.
const EDGE_PX = 8;
/// Letters typed within this long jump to the option they start.
const TYPEAHEAD_MS = 700;

interface Parts {
  button: HTMLButtonElement;
  labels: HTMLElement;
}

const enhanced = new WeakMap<HTMLSelectElement, Parts>();
let open: { select: HTMLSelectElement; list: HTMLElement; active: number } | null = null;
let typed = "";
let typedAt = 0;

/// The button shows every option's text stacked in one place, only the chosen one visible, so it
/// keeps the width of the longest and doesn't jump when the choice changes.
function paint(select: HTMLSelectElement) {
  const parts = enhanced.get(select);
  if (!parts) return;
  const chosen = Math.max(select.selectedIndex, 0);
  parts.labels.replaceChildren(
    ...[...select.options].map((o, i) => {
      const span = document.createElement("span");
      span.textContent = o.text;
      if (i === chosen) span.className = "current";
      return span;
    }),
  );
  parts.button.disabled = select.disabled;
  // Selects without a visible label name themselves; the button takes the same name.
  for (const attr of ["aria-label", "aria-labelledby", "title"]) {
    const value = select.getAttribute(attr);
    if (value === null) parts.button.removeAttribute(attr);
    else parts.button.setAttribute(attr, value);
  }
  if (open?.select === select) renderList();
}

function enhance(select: HTMLSelectElement) {
  if (enhanced.has(select) || select.multiple || select.size > 1) return;
  // A select put in place of an enhanced one (replaceWith) takes over its wrapper.
  let wrap = select.parentElement?.classList.contains("dropdown") ? select.parentElement : null;
  if (wrap) {
    wrap.querySelector(":scope > .dropdown-button")?.remove();
  } else {
    wrap = document.createElement("span");
    wrap.className = "dropdown";
    select.before(wrap);
    wrap.append(select);
  }
  select.classList.add("dropdown-native");
  select.tabIndex = -1;
  select.setAttribute("aria-hidden", "true");

  const button = document.createElement("button");
  button.type = "button";
  button.className = "dropdown-button";
  button.setAttribute("aria-haspopup", "listbox");
  button.setAttribute("aria-expanded", "false");
  const labels = document.createElement("span");
  labels.className = "dropdown-labels";
  button.append(labels);
  button.insertAdjacentHTML("beforeend", CHEVRON);
  // Before the select, so a wrapping <label> activates the button.
  wrap.insertBefore(button, select);

  button.addEventListener("click", () => (open?.select === select ? close(true) : show(select)));
  button.addEventListener("keydown", (e) => onKey(select, e));
  select.addEventListener("change", () => paint(select));

  // Options rebuilt, renamed or disabled by code show at once.
  new MutationObserver(() => paint(select)).observe(select, {
    childList: true,
    subtree: true,
    characterData: true,
    attributes: true,
    attributeFilter: ["disabled", "aria-label", "aria-labelledby", "title", "label", "selected"],
  });
  enhanced.set(select, { button, labels });
  paint(select);
}

function show(select: HTMLSelectElement) {
  close(false);
  const parts = enhanced.get(select);
  if (!parts || select.disabled || !select.options.length) return;
  const list = document.createElement("div");
  list.className = "dropdown-list";
  list.setAttribute("role", "listbox");
  list.tabIndex = -1;
  document.body.append(list);
  open = { select, list, active: Math.max(select.selectedIndex, 0) };
  parts.button.setAttribute("aria-expanded", "true");
  renderList();
  place(parts.button, list);
  list.querySelector<HTMLElement>(".active")?.scrollIntoView({ block: "nearest" });
}

function renderList() {
  if (!open) return;
  const { select, list } = open;
  list.replaceChildren(
    ...[...select.options].map((o, i) => {
      const item = document.createElement("div");
      item.className = `dropdown-option${i === open!.active ? " active" : ""}`;
      item.setAttribute("role", "option");
      item.setAttribute("aria-selected", String(i === select.selectedIndex));
      if (o.disabled) item.setAttribute("aria-disabled", "true");
      item.textContent = o.text;
      item.addEventListener("pointermove", () => setActive(i));
      // On pointerdown, before the button loses focus, so the choice lands first.
      item.addEventListener("pointerdown", (e) => {
        e.preventDefault();
        choose(i);
      });
      return item;
    }),
  );
}

/// Under the button, or above it when there's more room there; never past the window's edges.
function place(button: HTMLElement, list: HTMLElement) {
  const r = button.getBoundingClientRect();
  list.style.minWidth = `${r.width}px`;
  const below = window.innerHeight - r.bottom - EDGE_PX;
  const above = r.top - EDGE_PX;
  const height = list.scrollHeight;
  const up = height > below && above > below;
  list.style.maxHeight = `${Math.max(Math.min(up ? above : below, 320), 80)}px`;
  list.style.top = up ? "" : `${r.bottom + 4}px`;
  list.style.bottom = up ? `${window.innerHeight - r.top + 4}px` : "";
  const left = Math.min(r.left, window.innerWidth - list.offsetWidth - EDGE_PX);
  list.style.left = `${Math.max(left, EDGE_PX)}px`;
}

function setActive(i: number) {
  if (!open || i === open.active) return;
  open.active = i;
  open.list.querySelectorAll(".dropdown-option").forEach((el, j) => el.classList.toggle("active", j === i));
  open.list.children[i]?.scrollIntoView({ block: "nearest" });
}

function choose(i: number) {
  if (!open) return;
  const { select } = open;
  const option = select.options[i];
  close(true);
  if (!option || option.disabled || i === select.selectedIndex) return;
  select.selectedIndex = i;
  select.dispatchEvent(new Event("input", { bubbles: true }));
  select.dispatchEvent(new Event("change", { bubbles: true }));
}

function close(refocus: boolean) {
  if (!open) return;
  const { select, list } = open;
  open = null;
  list.remove();
  const button = enhanced.get(select)?.button;
  button?.setAttribute("aria-expanded", "false");
  if (refocus) button?.focus();
}

/// The next option from `from` in direction `step` that can be chosen.
function next(select: HTMLSelectElement, from: number, step: number): number {
  for (let i = from + step; i >= 0 && i < select.options.length; i += step) {
    if (!select.options[i].disabled) return i;
  }
  return from;
}

/// Adds `key` to what's been typed, and gives the first option that starts with it (-1 for none).
function typeAhead(select: HTMLSelectElement, key: string): number {
  typed = Date.now() - typedAt > TYPEAHEAD_MS ? key.toLowerCase() : typed + key.toLowerCase();
  typedAt = Date.now();
  return [...select.options].findIndex((o) => !o.disabled && o.text.toLowerCase().startsWith(typed));
}

function onKey(select: HTMLSelectElement, e: KeyboardEvent) {
  const isOpen = open?.select === select;
  const current = isOpen ? open!.active : Math.max(select.selectedIndex, 0);
  let target: number | null = null;
  switch (e.key) {
    case "ArrowDown":
      if (!isOpen && e.altKey) {
        e.preventDefault();
        show(select);
        return;
      }
      target = next(select, current, 1);
      break;
    case "ArrowUp":
      target = next(select, current, -1);
      break;
    case "Home":
      target = next(select, -1, 1);
      break;
    case "End":
      target = next(select, select.options.length, -1);
      break;
    case " ":
      // Mid type-ahead, a space is part of what's typed ("this o"), as in a native select.
      if (typed && Date.now() - typedAt <= TYPEAHEAD_MS) {
        target = typeAhead(select, e.key);
        if (target < 0) return;
        break;
      }
    // falls through
    case "Enter":
      e.preventDefault();
      if (isOpen) choose(open!.active);
      else show(select);
      return;
    case "Escape":
      if (!isOpen) return;
      e.preventDefault();
      e.stopPropagation();
      close(true);
      return;
    case "Tab":
      if (isOpen) close(false);
      return;
    default:
      if (e.key.length !== 1 || e.ctrlKey || e.metaKey || e.altKey) return;
      target = typeAhead(select, e.key);
      if (target < 0) return;
  }
  e.preventDefault();
  // Like a closed native select, the arrows change the choice; open, they move through the list.
  if (isOpen) setActive(target);
  else if (target !== select.selectedIndex) {
    select.selectedIndex = target;
    select.dispatchEvent(new Event("input", { bubbles: true }));
    select.dispatchEvent(new Event("change", { bubbles: true }));
  }
}

/// Setting a select's value or an option's selected from code fires no event, so the button
/// follows those setters too.
function followSetters() {
  const later = (select: HTMLSelectElement | null) => select && queueMicrotask(() => paint(select));
  for (const [proto, prop] of [
    [HTMLSelectElement.prototype, "value"],
    [HTMLSelectElement.prototype, "selectedIndex"],
    [HTMLOptionElement.prototype, "selected"],
  ] as const) {
    const desc = Object.getOwnPropertyDescriptor(proto, prop);
    if (!desc?.set) continue;
    Object.defineProperty(proto, prop, {
      ...desc,
      set(this: HTMLSelectElement | HTMLOptionElement, v: unknown) {
        desc.set!.call(this, v);
        later(this instanceof HTMLSelectElement ? this : this.parentElement instanceof HTMLSelectElement ? this.parentElement : null);
      },
    });
  }
}

/// What's on screen for `el`: a select's themed button, since the select itself is hidden; anything
/// else as it is. For code that scrolls to or focuses a control.
export function shownControl(el: HTMLElement): HTMLElement {
  return (el instanceof HTMLSelectElement && enhanced.get(el)?.button) || el;
}

/// Gives every select on the page, and every one added later, a themed dropdown. Call once.
export function useDropdowns() {
  followSetters();
  document.querySelectorAll("select").forEach(enhance);
  new MutationObserver((records) => {
    for (const r of records) {
      r.addedNodes.forEach((n) => {
        if (n instanceof HTMLSelectElement) enhance(n);
        else if (n instanceof Element) n.querySelectorAll("select").forEach(enhance);
      });
    }
    // A list whose select left the page goes with it.
    if (open && !open.select.isConnected) close(false);
  }).observe(document.body, { childList: true, subtree: true });

  document.addEventListener("pointerdown", (e) => {
    if (!open) return;
    const t = e.target as Node;
    if (open.list.contains(t) || enhanced.get(open.select)?.button.contains(t)) return;
    close(false);
  });
  // The list stays where it opened, so anything that moves the page closes it.
  document.addEventListener("scroll", (e) => open && !open.list.contains(e.target as Node) && close(false), true);
  window.addEventListener("resize", () => close(false));
  window.addEventListener("blur", () => close(false));
}
