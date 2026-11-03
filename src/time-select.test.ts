import { describe, expect, it } from "vitest";
import { replaceTimeInputs, timeSelect } from "./time-select";

describe("time dropdowns", () => {
  it("offer every quarter hour, with the value chosen", () => {
    const select = timeSelect("21:00", "Evening");
    expect(select.options.length).toBe(96);
    expect(select.value).toBe("21:00");
    expect(select.getAttribute("aria-label")).toBe("Evening");
  });

  it("keep a time off the quarter hours as its own choice, in order, set at once or later", () => {
    const select = timeSelect("09:07");
    expect(select.value).toBe("09:07");
    expect(select.options.length).toBe(97);
    expect(select.options[select.selectedIndex - 1].value).toBe("09:00");
    select.value = "21:10";
    expect(select.value).toBe("21:10");
    expect(select.options[select.selectedIndex + 1].value).toBe("21:15");
  });

  it("take the place of time fields, keeping their id, class and value", () => {
    document.body.innerHTML = `<label>At <input type="time" id="evening-time" class="x" value="21:30" /></label>`;
    replaceTimeInputs();
    const select = document.getElementById("evening-time") as HTMLSelectElement;
    expect(select.tagName).toBe("SELECT");
    expect(select.classList.contains("x")).toBe(true);
    expect(select.value).toBe("21:30");
    expect(document.querySelector('input[type="time"]')).toBeNull();
  });
});
