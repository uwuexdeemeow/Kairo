import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { askable, clock, mountScan, nameOf, verdictOf, type ScanActions } from "./scan-view";
import type { VtAnswer } from "./virustotal-api";

const verdict = (malicious: number, suspicious = 0, extra: Partial<VtAnswer & { state: "verdict" }> = {}): VtAnswer => ({
  state: "verdict",
  malicious,
  suspicious,
  total: 71,
  names: malicious ? [["Kaspersky", "Trojan.Generic"], ["ESET", "Win32.Agent"]] : [],
  more: malicious > 2 ? 1 : 0,
  lastMs: new Date(2026, 10, 4).getTime(),
  link: "https://www.virustotal.com/gui/file/abc",
  title: null,
  then: "file:abc",
  ...extra,
});

let root: HTMLElement;
let act: { [K in keyof ScanActions]: ReturnType<typeof vi.fn> };

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
  document.body.innerHTML = `<div id="root"></div>`;
  root = document.getElementById("root")!;
  act = {
    hasKey: vi.fn(async () => true),
    checkFile: vi.fn(async () => verdict(3)),
    check: vi.fn(async () => verdict(0)),
    upload: vi.fn(),
    scanLink: vi.fn(),
    follow: vi.fn(),
    rescan: vi.fn(),
    pickFile: vi.fn(async () => "C:\\Users\\a\\Downloads\\setup.exe"),
    openReport: vi.fn(),
    openSettings: vi.fn(),
  };
});
afterEach(() => vi.useRealTimers());

const results = () => [...root.querySelectorAll<HTMLElement>(".scan-result")];
const first = () => results()[0];
const field = () => root.querySelector<HTMLInputElement>(".scan-query")!;
const checkTyped = (text: string) => {
  field().value = text;
  root.querySelector<HTMLButtonElement>(".scan-check")!.click();
};

describe("the scan window", () => {
  it("checks a chosen file by its hash, and shows the verdict worst first", async () => {
    const scan = await mountScan(root, act);
    root.querySelector<HTMLButtonElement>(".scan-choose")!.click();
    await vi.waitFor(() => expect(act.checkFile).toHaveBeenCalledWith("C:\\Users\\a\\Downloads\\setup.exe"));
    await vi.waitFor(() => expect(first().textContent).toContain("Malicious 3 of 71 engines flag it."));
    expect(first().querySelector(".scan-name")!.textContent).toBe("setup.exe");
    expect(first().classList.contains("bad")).toBe(true);
    expect(first().querySelector(".scan-advice")!.textContent).toContain("Don't open it.");
    expect(first().textContent).toContain("Trojan.Generic · Kaspersky");
    expect(first().textContent).toContain("+1 more");
    first().querySelector<HTMLButtonElement>(".scan-report")!.click();
    expect(act.openReport).toHaveBeenCalledWith("https://www.virustotal.com/gui/file/abc");
    void scan;
  });

  it("colours a clean answer green and a suspicious one amber", async () => {
    const scan = await mountScan(root, act);
    scan.check("https://example.com");
    await vi.waitFor(() => expect(first().textContent).toContain("Nothing flagged None of the 71 engines flags it."));
    expect(first().classList.contains("clean")).toBe(true);
    expect(first().querySelector(".scan-name")!.textContent).toBe("example.com");
    act.check.mockResolvedValueOnce(verdict(0, 2));
    scan.check("44d88612fea8a8f36de82e1278abb02f");
    await vi.waitFor(() => expect(first().classList.contains("odd")).toBe(true));
    expect(first().querySelector(".scan-name")!.textContent).toBe("44d88612fea8…");
  });

  it("says what isn't a link or a hash under the field, keeps the text and sends nothing", async () => {
    await mountScan(root, act);
    checkTyped("hello there");
    expect(act.check).not.toHaveBeenCalled();
    expect(results()).toHaveLength(0);
    expect(field().value).toBe("hello there");
    expect(field().getAttribute("aria-invalid")).toBe("true");
    expect(root.querySelector(".scan-hint")!.textContent).toBe("That isn't a link or a hash. A link looks like example.com/page.");
    expect(root.querySelector(".sr-only[aria-live]")!.textContent).toBe("That isn't a link or a hash.");
    // Typing on takes the line away, and a link goes through.
    field().dispatchEvent(new Event("input"));
    expect(root.querySelector(".scan-hint")!.textContent).toBe("");
    checkTyped("example.com/page");
    await vi.waitFor(() => expect(act.check).toHaveBeenCalledWith("example.com/page"));
    expect(field().value).toBe("");
  });

  it("knows a hash by its length, and a bare host by its ending", () => {
    for (const ok of ["a".repeat(32), "A".repeat(40), "f".repeat(64), "https://x.test/a%20b", "example.com", "http://example.com", " example.com/page "]) expect(askable(ok), ok).toBe(true);
    for (const bad of ["a".repeat(33), "example", "no space.com", "foo.1", "ftp://example.com x", "", "   "]) expect(askable(bad), bad).toBe(false);
  });

  it("looks a link up first, and asks before sending one VirusTotal hasn't seen", async () => {
    act.check.mockResolvedValueOnce({ state: "linkNotSeen", url: "https://example.com/reset?token=abc" });
    act.scanLink.mockResolvedValueOnce({ state: "scanning", analysis: "an9", then: "url:xyz" });
    act.follow.mockResolvedValueOnce(verdict(0));
    const scan = await mountScan(root, act);
    scan.check("https://example.com/reset?token=abc");
    await vi.waitFor(() => expect(first().textContent).toContain("VirusTotal hasn't seen this link."));
    // Said plainly: scanning sends it, and links can carry private tokens.
    expect(first().textContent).toContain("Scanning sends it to VirusTotal");
    expect(first().textContent).toContain("Only scan what you'd be fine with others seeing.");
    expect(act.scanLink).not.toHaveBeenCalled();
    first().querySelector<HTMLButtonElement>(".scan-send")!.click();
    await vi.waitFor(() => expect(act.scanLink).toHaveBeenCalledWith("https://example.com/reset?token=abc"));
    await vi.waitFor(() => expect(first().textContent).toContain("Scanning on VirusTotal… 0:00"));
  });

  it("leaves a link that isn't sent, and says so", async () => {
    act.check.mockResolvedValueOnce({ state: "linkNotSeen", url: "https://example.com/a" });
    const scan = await mountScan(root, act);
    scan.check("example.com/a");
    await vi.waitFor(() => expect(first().textContent).toContain("VirusTotal hasn't seen this link."));
    first().querySelector<HTMLButtonElement>(".scan-not-now")!.click();
    expect(first().textContent).toContain("Not sent.");
    expect(act.scanLink).not.toHaveBeenCalled();
  });

  it("always says where a link or a file goes, beside the field", async () => {
    await mountScan(root, act);
    expect(root.querySelector(".scan-privacy")!.textContent).toBe("A link or hash is only looked up. VirusTotal is sent a link, or a file, only if you choose to scan it.");
  });

  it("offers Try again only for what trying again can fix, and a way to take a result away", async () => {
    const scan = await mountScan(root, act);
    act.check.mockRejectedValueOnce("That's not a link or a hash.");
    scan.check("https://example.com");
    await vi.waitFor(() => expect(first().classList.contains("error")).toBe(true));
    expect(first().querySelector(".scan-retry")).toBeNull();
    first().querySelector<HTMLButtonElement>(".scan-remove")!.click();
    expect(results()).toHaveLength(0);
    expect(root.querySelector(".sr-only[aria-live]")!.textContent).toBe("Removed.");
    act.check.mockRejectedValueOnce("VirusTotal answered 502. Try again in a moment.");
    scan.check("https://example.org");
    await vi.waitFor(() => expect(first().querySelector(".scan-retry")).not.toBeNull());
  });

  it("lets a result waiting its turn be cancelled", async () => {
    const scan = await mountScan(root, act);
    // Four requests use the free minute; the fifth waits.
    for (let i = 0; i < 4; i++) scan.check(`https://example.com/${i}`);
    scan.check("https://example.com/five");
    await vi.waitFor(() => expect(first().querySelector(".scan-time[data-until]")).not.toBeNull());
    first().querySelector<HTMLButtonElement>(".scan-cancel")!.click();
    expect(results().some((r) => r.textContent?.includes("example.com/five"))).toBe(false);
  });

  it("asks before uploading a file VirusTotal hasn't seen", async () => {
    act.checkFile.mockResolvedValueOnce({ state: "notSeen", sha256: "f".repeat(64), size: 1000, tooBig: false });
    act.upload.mockResolvedValueOnce({ state: "scanning", analysis: "an1", then: "file:fff" });
    act.follow.mockResolvedValueOnce({ state: "scanning", analysis: "an1", then: "file:fff" }).mockResolvedValueOnce(verdict(0));
    const scan = await mountScan(root, act);
    scan.file("/home/a/notes.docx");
    await vi.waitFor(() => expect(first().textContent).toContain("VirusTotal hasn't seen this file."));
    expect(first().textContent).toContain("Uploading shares it with security companies.");
    expect(act.upload).not.toHaveBeenCalled();
    first().querySelector<HTMLButtonElement>(".scan-upload")!.click();
    await vi.waitFor(() => expect(act.upload).toHaveBeenCalledWith("/home/a/notes.docx"));
    await vi.waitFor(() => expect(first().textContent).toContain("Scanning on VirusTotal… 0:00"));
    // Asked again every 20 seconds until it's done.
    await vi.advanceTimersByTimeAsync(20_000);
    expect(act.follow).toHaveBeenCalledWith("an1", "file:fff");
    await vi.advanceTimersByTimeAsync(20_000);
    await vi.waitFor(() => expect(first().textContent).toContain("Nothing flagged"));
  });

  it("leaves a file that isn't uploaded, and says when one is too big", async () => {
    act.checkFile.mockResolvedValueOnce({ state: "notSeen", sha256: "f".repeat(64), size: 1000, tooBig: false }).mockResolvedValueOnce({ state: "notSeen", sha256: "e".repeat(64), size: 40e6, tooBig: true });
    const scan = await mountScan(root, act);
    scan.file("/a/one.bin");
    await vi.waitFor(() => expect(first().querySelector(".scan-not-now")).not.toBeNull());
    first().querySelector<HTMLButtonElement>(".scan-not-now")!.click();
    expect(first().textContent).toContain("Not uploaded.");
    scan.file("/a/two.iso");
    await vi.waitFor(() => expect(first().textContent).toContain("bigger than the 32 MB a free key can upload"));
    expect(first().querySelector(".scan-upload")).toBeNull();
  });

  it("tells screen readers only the result that changed", async () => {
    const scan = await mountScan(root, act);
    expect(root.querySelector(".scan-results")!.hasAttribute("aria-live")).toBe(false);
    const said = () => root.querySelector(".sr-only[aria-live]")!.textContent;
    scan.file("/x/setup.exe");
    await vi.waitFor(() => expect(said()).toBe("setup.exe: Malicious. 3 of 71 engines flag it."));
    scan.check("https://example.com");
    await vi.waitFor(() => expect(said()).toBe("example.com: Nothing flagged. None of the 71 engines flags it."));
  });

  it("says when a scan is taking a while, and can stop following it and pick it up again", async () => {
    act.check.mockResolvedValueOnce({ state: "scanning", analysis: "an2", then: "url:x" });
    act.follow.mockResolvedValue({ state: "scanning", analysis: "an2", then: "url:x" });
    vi.useRealTimers();
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });
    const scan = await mountScan(root, act);
    scan.check("https://slow.example");
    await vi.waitFor(() => expect(first().textContent).toContain("Scanning on VirusTotal… 0:00"));
    await vi.advanceTimersByTimeAsync(41_000);
    expect(first().querySelector(".scan-time")!.textContent).toMatch(/^Scanning on VirusTotal… 0:4\d$/);
    await vi.advanceTimersByTimeAsync(50_000);
    expect(first().querySelector(".scan-time")!.textContent).toMatch(/^Taking longer than usual\. Kairo keeps checking\. 1:3\d$/);
    const asked = act.follow.mock.calls.length;
    first().querySelector<HTMLButtonElement>(".scan-stop")!.click();
    expect(first().textContent).toContain("Stopped.");
    await vi.advanceTimersByTimeAsync(60_000);
    expect(act.follow.mock.calls.length).toBe(asked);
    act.follow.mockResolvedValueOnce(verdict(1));
    first().querySelector<HTMLButtonElement>(".scan-again")!.click();
    await vi.waitFor(() => expect(first().textContent).toContain("Probably fine 1 of 71 engines flags it, often a false alarm."));
  });

  it("keeps the last ten, newest first", async () => {
    vi.useRealTimers();
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date"] });
    const scan = await mountScan(root, act);
    for (let i = 0; i < 12; i++) scan.check(`https://site${i}.example`);
    // Four a minute: the rest wait their turn, kept on the list until they're done.
    await vi.waitFor(() => expect(root.textContent).toContain("Queued: free limit of 4 a minute"));
    expect(act.check).toHaveBeenCalledTimes(4);
    await vi.advanceTimersByTimeAsync(3 * 60_000);
    expect(act.check).toHaveBeenCalledTimes(12);
    await vi.waitFor(() => expect(results().length).toBe(10));
    expect(first().querySelector(".scan-name")!.textContent).toBe("site11.example");
  });

  it("keeps the field focused when the window comes back and the key hasn't changed", async () => {
    const scan = await mountScan(root, act);
    field().focus();
    await scan.refreshKey();
    expect(document.activeElement).toBe(field());
  });

  it("moves something checked again to the top instead of adding it twice", async () => {
    const scan = await mountScan(root, act);
    scan.check("https://example.com/a");
    scan.check("https://other.example");
    await vi.waitFor(() => expect(results().length).toBe(2));
    scan.check("https://example.com/a");
    await vi.waitFor(() => expect(first().querySelector(".scan-name")!.textContent).toBe("example.com/a"));
    expect(results().length).toBe(2);
  });

  it("says it couldn't check, with a way to try again and to settings for a key problem", async () => {
    await mountScan(root, act);
    act.check.mockRejectedValueOnce("VirusTotal didn't accept the key. Check it in Kairo's settings.");
    checkTyped("https://example.com");
    await vi.waitFor(() => expect(first().classList.contains("error")).toBe(true));
    expect(first().textContent).toContain("Couldn't check");
    first().querySelector<HTMLButtonElement>(".scan-settings-link")!.click();
    expect(act.openSettings).toHaveBeenCalled();
    first().querySelector<HTMLButtonElement>(".scan-retry")!.click();
    await vi.waitFor(() => expect(first().textContent).toContain("Nothing flagged"));
    expect(act.check).toHaveBeenCalledTimes(2);
  });

  it("keeps the keyboard on the control it was on as other results change, and lets a newer error not take the lead", async () => {
    await mountScan(root, act);
    checkTyped("https://good.example");
    await vi.waitFor(() => expect(first().textContent).toContain("Nothing flagged"));
    act.check.mockRejectedValueOnce("Couldn't reach VirusTotal. Check your connection and try again.");
    checkTyped("https://offline.example");
    await vi.waitFor(() => expect(first().classList.contains("error")).toBe(true));
    // The verdict below is still the figure; the error above isn't one.
    expect(first().classList.contains("lead")).toBe(false);
    expect(results()[1].classList.contains("lead")).toBe(true);
    first().querySelector<HTMLButtonElement>(".scan-retry")!.focus();
    // Another result arrives and every card is drawn again.
    let answer: (v: VtAnswer) => void = () => {};
    act.check.mockImplementationOnce(() => new Promise<VtAnswer>((r) => (answer = r)));
    checkTyped("https://third.example");
    await vi.waitFor(() => expect(act.check).toHaveBeenCalledTimes(3));
    answer(verdict(0));
    await vi.waitFor(() => expect(results().length).toBe(3));
    await vi.waitFor(() => expect(first().textContent).toContain("Nothing flagged"));
    expect(document.activeElement?.classList.contains("scan-retry")).toBe(true);
    expect(document.activeElement?.closest<HTMLElement>(".scan-result")?.textContent).toContain("offline.example");
  });

  it("calls a hash VirusTotal hasn't seen unknown, and names a known one as VirusTotal does", async () => {
    const scan = await mountScan(root, act);
    act.check.mockResolvedValueOnce({ state: "unknown" });
    scan.check("a".repeat(64));
    await vi.waitFor(() => expect(first().textContent).toContain("Unknown to VirusTotal"));
    expect(first().classList.contains("error")).toBe(false);
    act.check.mockResolvedValueOnce(verdict(0, 0, { title: "installer.msi" }));
    scan.check("b".repeat(64));
    await vi.waitFor(() => expect(first().querySelector(".scan-name")!.textContent).toBe("installer.msi"));
  });

  it("scans an answer again on asking", async () => {
    act.rescan.mockResolvedValueOnce(verdict(0));
    const scan = await mountScan(root, act);
    scan.file("/x/setup.exe");
    await vi.waitFor(() => expect(first().querySelector(".scan-rescan")).not.toBeNull());
    first().querySelector<HTMLButtonElement>(".scan-rescan")!.click();
    await vi.waitFor(() => expect(act.rescan).toHaveBeenCalledWith("file:abc"));
    await vi.waitFor(() => expect(first().textContent).toContain("Nothing flagged"));
  });

  it("says when there's nothing to check, and names its results for screen readers", async () => {
    await mountScan(root, act);
    // The field has the keyboard as the window opens.
    expect(document.activeElement).toBe(field());
    checkTyped("   ");
    expect(field().getAttribute("aria-invalid")).toBe("true");
    // Said in the field too, for those who see it, until something's typed.
    expect(field().placeholder).toBe("Paste a link or a hash first");
    expect(root.querySelector(".sr-only[aria-live]")!.textContent).toBe("Paste a link or a hash first.");
    expect(act.check).not.toHaveBeenCalled();
    expect(root.querySelector("h1")!.textContent).toBe("Check a file or link with VirusTotal");
    checkTyped("https://example.com");
    await vi.waitFor(() => expect(first().textContent).toContain("Nothing flagged"));
    const name = first().querySelector(".scan-name")!;
    expect(first().getAttribute("aria-labelledby")).toBe(name.id);
    expect(first().querySelector(".scan-report [aria-hidden]")!.textContent).toBe(" ↗");
  });

  it("without a key, says where to add one", async () => {
    act.hasKey.mockResolvedValue(false);
    await mountScan(root, act);
    expect(root.textContent).toContain("Add your VirusTotal key in Kairo's settings");
    root.querySelector<HTMLButtonElement>(".scan-settings")!.click();
    expect(act.openSettings).toHaveBeenCalled();
    expect(root.querySelector(".scan-query")).toBeNull();
  });
});

describe("a verdict in words", () => {
  const v = (malicious: number, suspicious = 0, total = 71) => verdictOf({ ...(verdict(malicious, suspicious) as VtAnswer & { state: "verdict" }), total });
  it("is clean with nothing flagged, and calm about a lone engine", () => {
    expect(v(0)).toMatchObject({ tone: "clean", word: "Nothing flagged" });
    expect(v(1)).toMatchObject({ tone: "fine", word: "Probably fine" });
    expect(v(0, 1)).toMatchObject({ tone: "fine", word: "Probably fine" });
  });

  it("is malicious when two or more engines say so, and suspicious otherwise", () => {
    expect(v(2)).toMatchObject({ tone: "bad", word: "Malicious" });
    expect(v(1, 1)).toMatchObject({ tone: "odd", word: "Suspicious" });
    expect(v(0, 4)).toMatchObject({ tone: "odd", word: "Suspicious" });
    expect(v(5).advice).toBeTruthy();
    expect(v(0).advice).toBeUndefined();
  });

  it("says when no engine has answered, rather than calling it clean", () => {
    expect(v(0, 0, 0)).toMatchObject({ tone: "none", word: "No answer yet" });
  });

  it("times a scan in minutes and seconds", () => {
    expect(clock(0)).toBe("0:00");
    expect(clock(91_500)).toBe("1:31");
  });
});

describe("a result's name", () => {
  it("keeps a link's path, so two pages on one site aren't the same", () => {
    expect(nameOf("https://example.com/a", false)).toBe("example.com/a");
    expect(nameOf("example.com/", false)).toBe("example.com");
    expect(nameOf("C:\\Users\\a\\setup.exe", true)).toBe("setup.exe");
  });
});
