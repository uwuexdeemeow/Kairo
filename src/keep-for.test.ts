import { beforeEach, describe, expect, it, vi } from "vitest";
import { keepFor } from "./keep-for";

const labels = { "0": "forever", "365": "1 year", "30": "30 days", "7": "7 days" };
let select: HTMLSelectElement;
let apply: HTMLButtonElement;
let status: HTMLElement;
let save: ReturnType<typeof vi.fn>;
let control: ReturnType<typeof keepFor>;

const choose = (value: string) => {
  select.value = value;
  select.dispatchEvent(new Event("change"));
};

beforeEach(() => {
  document.body.innerHTML = `<select id="s"><option value="0">Forever</option><option value="365">1 year</option><option value="30">30 days</option><option value="7">7 days</option></select><button id="a" hidden></button><span id="t"></span>`;
  select = document.getElementById("s") as HTMLSelectElement;
  apply = document.getElementById("a") as HTMLButtonElement;
  status = document.getElementById("t")!;
  save = vi.fn(async () => 0);
  control = keepFor({ select, apply, status, labels, applyText: (l) => `Save and clear older than ${l}`, armedText: "Click again to clear", changedText: "Saved. Cleared.", save });
});

describe("a Keep for choice", () => {
  it("shows the saved choice", () => {
    control.render(365);
    expect(select.value).toBe("365");
    expect(apply.hidden).toBe(true);
  });

  it("saves a longer time, or forever, at once", async () => {
    control.render(30);
    choose("365");
    await vi.waitFor(() => expect(save).toHaveBeenCalledWith(365));
    await vi.waitFor(() => expect(status.textContent).toBe("Saved."));
    expect(apply.hidden).toBe(true);
    choose("0");
    await vi.waitFor(() => expect(save).toHaveBeenLastCalledWith(0));
  });

  it("makes a shorter time wait for a confirmed click, since it deletes", async () => {
    control.render(365);
    choose("30");
    expect(save).not.toHaveBeenCalled();
    expect(apply.hidden).toBe(false);
    expect(apply.textContent).toBe("Save and clear older than 30 days");
    // The first click arms it, the second saves.
    apply.click();
    expect(apply.textContent).toBe("Click again to clear");
    expect(save).not.toHaveBeenCalled();
    save.mockResolvedValueOnce(12);
    apply.click();
    await vi.waitFor(() => expect(save).toHaveBeenCalledWith(30));
    await vi.waitFor(() => expect(status.textContent).toBe("Saved. Cleared."));
    expect(apply.hidden).toBe(true);
    expect(select.value).toBe("30");
  });

  it("counts anything shorter than forever as shorter, and going back puts the button away", async () => {
    control.render(0);
    choose("365");
    expect(apply.hidden).toBe(false);
    expect(apply.textContent).toBe("Save and clear older than 1 year");
    // Changing their mind to something longer than what's saved saves it and hides the button.
    control.render(30);
    choose("365");
    await vi.waitFor(() => expect(save).toHaveBeenCalledWith(365));
    expect(apply.hidden).toBe(true);
  });

  it("says why when it can't save, and keeps the button for another try", async () => {
    control.render(365);
    choose("7");
    save.mockRejectedValueOnce("database busy");
    apply.click();
    apply.click();
    await vi.waitFor(() => expect(status.textContent).toContain("Couldn't save."));
    // The button stays, for another try.
    expect(apply.hidden).toBe(false);
  });
});
