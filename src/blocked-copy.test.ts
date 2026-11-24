import { describe, expect, it } from "vitest";
import { confirmNote, FAILED_NOTE, holdNote, limitLine, PAUSE } from "./blocked-copy";

describe("the blocked window's words", () => {
  it("says what strict mode is doing while it holds a button back", () => {
    expect(holdNote("allow", 12_000)).toBe("Strict mode is on. You can ask again in 12 seconds.");
    expect(holdNote("allow", 1)).toBe("Strict mode is on. You can ask again in 1 second.");
    expect(holdNote("end", 4_200)).toBe("Strict mode is on. You can end the session in 5 seconds.");
  });

  it("asks to confirm once the wait is over, naming what it will do", () => {
    expect(confirmNote("allow", "Allow 5 minutes")).toBe("Confirm to allow 5 minutes.");
    // Past a limit the label is already a noun phrase, so it isn't lowercased into 'Confirm: 5 more minutes'.
    expect(confirmNote("allow", "5 more minutes")).toBe("Confirm to get 5 more minutes.");
    expect(confirmNote("end", "End session")).toBe("Confirm to end the session.");
  });

  it("asks only for a wait before a distracting app outside a session, not for strict mode", () => {
    expect(holdNote("allow", 7_200, true)).toBe("Wait 8 seconds, then confirm if you still want it.");
    expect(holdNote("allow", 1, true)).toBe("Wait 1 second, then confirm if you still want it.");
    expect(holdNote("allow", 12_000)).toContain("Strict mode");
    expect(confirmNote("allow", PAUSE.allow)).toBe("Confirm to open it for 15 minutes.");
    expect(PAUSE.title("Discord")).toBe("Still want Discord?");
    expect(PAUSE.detail).not.toContain("blocked");
  });

  it("says a failure plainly, and a limit as the user's own", () => {
    expect(FAILED_NOTE).toBe("Couldn’t do that. Try again.");
    expect(limitLine()).toBe("You’ve reached the limit you set for it today, so it stays blocked until midnight.");
  });
});
