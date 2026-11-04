import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mountScan, type ScanActions } from "./scan-view";
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
    follow: vi.fn(),
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
    await vi.waitFor(() => expect(first().textContent).toContain("3 of 71 engines flag it"));
    expect(first().querySelector(".scan-name")!.textContent).toBe("setup.exe");
    expect(first().classList.contains("bad")).toBe(true);
    expect(first().textContent).toContain("Trojan.Generic · Kaspersky");
    expect(first().textContent).toContain("+1 more");
    first().querySelector<HTMLButtonElement>(".scan-report")!.click();
    expect(act.openReport).toHaveBeenCalledWith("https://www.virustotal.com/gui/file/abc");
    void scan;
  });

  it("colours a clean answer green and a suspicious one amber", async () => {
    const scan = await mountScan(root, act);
    scan.check("https://example.com");
    await vi.waitFor(() => expect(first().textContent).toContain("0 of 71 engines flag it"));
    expect(first().classList.contains("clean")).toBe(true);
    expect(first().querySelector(".scan-name")!.textContent).toBe("example.com");
    act.check.mockResolvedValueOnce(verdict(0, 2));
    scan.check("44d88612fea8a8f36de82e1278abb02f");
    await vi.waitFor(() => expect(first().classList.contains("odd")).toBe(true));
    expect(first().querySelector(".scan-name")!.textContent).toBe("44d88612fea8…");
  });

  it("checks what's typed in its field, and says why it couldn't", async () => {
    await mountScan(root, act);
    act.check.mockRejectedValueOnce("That's not a link or a hash.");
    checkTyped("hello there");
    await vi.waitFor(() => expect(first().textContent).toContain("That's not a link or a hash."));
    expect(first().classList.contains("error")).toBe(true);
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
    await vi.waitFor(() => expect(first().textContent).toContain("Scanning… usually under a minute"));
    // Asked again every 20 seconds until it's done.
    await vi.advanceTimersByTimeAsync(20_000);
    expect(act.follow).toHaveBeenCalledWith("an1", "file:fff");
    await vi.advanceTimersByTimeAsync(20_000);
    await vi.waitFor(() => expect(first().textContent).toContain("0 of 71 engines flag it"));
  });

  it("leaves a file that isn't uploaded, and says when one is too big", async () => {
    act.checkFile.mockResolvedValueOnce({ state: "notSeen", sha256: "f".repeat(64), size: 1000, tooBig: false }).mockResolvedValueOnce({ state: "notSeen", sha256: "e".repeat(64), size: 40e6, tooBig: true });
    const scan = await mountScan(root, act);
    scan.file("/a/one.bin");
    await vi.waitFor(() => expect(first().querySelector(".scan-not-now")).not.toBeNull());
    first().querySelector<HTMLButtonElement>(".scan-not-now")!.click();
    expect(first().textContent).toContain("Not uploaded.");
    scan.file("/a/two.iso");
    await vi.waitFor(() => expect(first().textContent).toContain("Too big to upload with a free key."));
    expect(first().querySelector(".scan-upload")).toBeNull();
  });

  it("tells screen readers only the result that changed", async () => {
    const scan = await mountScan(root, act);
    expect(root.querySelector(".scan-results")!.hasAttribute("aria-live")).toBe(false);
    const said = () => root.querySelector(".sr-only[aria-live]")!.textContent;
    scan.file("/x/setup.exe");
    await vi.waitFor(() => expect(said()).toBe("setup.exe: 3 of 71 engines flag it"));
    scan.check("https://example.com");
    await vi.waitFor(() => expect(said()).toBe("example.com: 0 of 71 engines flag it"));
  });

  it("keeps the last ten, newest first", async () => {
    const scan = await mountScan(root, act);
    for (let i = 0; i < 12; i++) scan.check(`https://site${i}.example`);
    await vi.waitFor(() => expect(results().length).toBe(10));
    expect(first().querySelector(".scan-name")!.textContent).toBe("site11.example");
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
