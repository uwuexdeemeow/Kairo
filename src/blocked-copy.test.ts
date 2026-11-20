import { describe, expect, it } from "vitest";
import { confirmNote, FAILED_NOTE, holdNote, limitLine } from "./blocked-copy";

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

  it("says a failure plainly, and a limit as the user's own", () => {
    expect(FAILED_NOTE).toBe("Couldn’t do that. Try again.");
    expect(limitLine()).toBe("You’ve reached the limit you set for it today, so it stays blocked until midnight.");
  });
});
