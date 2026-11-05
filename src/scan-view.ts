import { h } from "./ui";
import type { VtAnswer, VtVerdict } from "./virustotal-api";

/// The scan window's page: a file dropped or chosen, or a link or hash typed, checked against
/// VirusTotal, the results stacked newest first. A file is looked up by its hash; it's uploaded
/// only when VirusTotal hasn't seen it and the user says so.

export type ScanActions = {
  hasKey(): Promise<boolean>;
  checkFile(path: string): Promise<VtAnswer>;
  check(text: string): Promise<VtAnswer>;
  upload(path: string): Promise<VtAnswer>;
  follow(analysis: string, then: string): Promise<VtAnswer>;
  pickFile(): Promise<string | null>;
  openReport(link: string): void;
  openSettings(): void;
};

export type ScanView = {
  /** Checks a link or hash. */
  check(text: string): void;
  /** Checks a file, by its hash. */
  file(path: string): void;
  /** Reads again whether there's a key, as after it was added in settings. */
  refreshKey(): Promise<void>;
};

/// A scan still going is asked about this often: the free key allows four lookups a minute.
export const FOLLOW_MS = 20_000;
/// The results kept on screen.
const KEEP = 10;

/// A scan going on longer than this is said to be taking a while.
const SLOW_MS = 90_000;

type Entry = {
  name: string;
  /** The file, for an upload. */
  path?: string;
  /** When VirusTotal started scanning it, for the time shown while it does. */
  since?: number;
  /** Counts each run, so one that was stopped drops what it hears afterwards. */
  run?: number;
  state:
    | { kind: "checking" }
    | { kind: "answer"; answer: VtAnswer }
    | { kind: "error"; why: string }
    | { kind: "skipped" }
    /** No longer followed; VirusTotal carries on, and it can be asked again. */
    | { kind: "stopped"; analysis: string; then: string };
};

/// What a result is called: a file's name, a link's host, or a hash cut short.
export function nameOf(text: string, isFile: boolean): string {
  if (isFile) return text.split(/[\\/]/).pop() || text;
  const t = text.trim();
  if (/^[0-9a-f]{32,64}$/i.test(t)) return `${t.slice(0, 12)}…`;
  try {
    return new URL(/^https?:\/\//i.test(t) ? t : `https://${t}`).host || t;
  } catch {
    return t;
  }
}

/// A verdict in words: the one to read first, the count behind it, and what to do when it's bad.
/// A single engine on its own is often a false alarm, so it's said calmly; two or more calling
/// it malicious make it malicious, and anything else flagged is suspicious.
export function verdictOf(v: VtVerdict): { tone: "bad" | "odd" | "fine" | "clean" | "none"; word: string; count: string; advice?: string } {
  const flagged = v.malicious + v.suspicious;
  if (!v.total) return { tone: "none", word: "No answer yet", count: "No engine has given a result for it." };
  if (!flagged) return { tone: "clean", word: "Clean", count: `None of the ${v.total} engines flags it.` };
  if (flagged === 1) return { tone: "fine", word: "Probably fine", count: `1 of ${v.total} engines flags it, often a false alarm.` };
  const count = `${flagged} of ${v.total} engines flag it.`;
  if (v.malicious >= 2) return { tone: "bad", word: "Malicious", count, advice: "Don't open it. The full report says what the engines found." };
  return { tone: "odd", word: "Suspicious", count, advice: "Open it only if you trust where it came from." };
}

/// Minutes and seconds, as a scan's time so far.
export const clock = (ms: number) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

/// What a scan going on says: how long so far, and that it's taking a while past a minute and a half.
const scanningText = (since: number, now: number) =>
  now - since >= SLOW_MS ? `Taking longer than usual. Kairo keeps checking. ${clock(now - since)}` : `Scanning on VirusTotal… ${clock(now - since)}`;

const day = (ms: number) => new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });

export async function mountScan(root: HTMLElement, act: ScanActions): Promise<ScanView> {
  const entries: Entry[] = [];
  const list = h("div", { class: "scan-results" });
  // Screen readers hear the result that just changed, once, rather than the whole list again.
  const said = h("p", { class: "sr-only", "aria-live": "polite" });
  const query = h("input", { class: "filter scan-query", type: "text", placeholder: "Paste a link or a hash", "aria-label": "A link or a hash", autocomplete: "off", spellcheck: "false" }) as HTMLInputElement;
  const checkButton = h("button", { class: "btn primary scan-check" }, "Check");
  const choose = h("button", { class: "btn scan-choose" }, "Choose file…");
  const drop = h("div", { class: "scan-drop" }, h("p", {}, "Drop a file here"), h("p", { class: "muted" }, "Only its fingerprint is sent, unless you choose to upload it."), choose);

  const draw = () => {
    list.replaceChildren(...entries.map(drawEntry));
  };
  // A scan's time ticks in place, so nothing else on the page is redrawn under the keyboard.
  window.setInterval(() => {
    for (const el of list.querySelectorAll<HTMLElement>(".scan-elapsed")) el.textContent = scanningText(Number(el.dataset.since), Date.now());
  }, 1000);

  const drawEntry = (e: Entry): HTMLElement => {
    const head = h("div", { class: "scan-head" }, h("strong", { class: "scan-name" }, e.name));
    const s = e.state;
    if (s.kind === "checking") return h("article", { class: "scan-result" }, head, h("p", { class: "muted" }, "Checking…"));
    if (s.kind === "error") return h("article", { class: "scan-result error" }, head, h("p", {}, s.why));
    if (s.kind === "skipped") return h("article", { class: "scan-result" }, head, h("p", { class: "muted" }, "Not uploaded."));
    if (s.kind === "stopped") {
      const again = h("button", { class: "link-btn scan-again" }, "Check again");
      again.addEventListener("click", () => void run(e, () => act.follow(s.analysis, s.then)));
      return h("article", { class: "scan-result" }, head, h("p", { class: "muted" }, "Stopped. VirusTotal carries on scanning it."), h("p", { class: "scan-foot" }, again));
    }
    const a = s.answer;
    if (a.state === "scanning") {
      const since = e.since ?? Date.now();
      const stop = h("button", { class: "link-btn scan-stop" }, "Stop");
      stop.addEventListener("click", () => {
        e.run = (e.run ?? 0) + 1;
        e.since = undefined;
        e.state = { kind: "stopped", analysis: a.analysis, then: a.then };
        changed(e);
      });
      return h("article", { class: "scan-result" }, head, h("p", { class: "muted scan-elapsed", "data-since": String(since) }, scanningText(since, Date.now())), h("p", { class: "scan-foot" }, stop));
    }
    if (a.state === "notSeen") {
      if (a.tooBig) return h("article", { class: "scan-result" }, head, h("p", {}, "VirusTotal hasn't seen this file, and it's bigger than the 32 MB a free key can upload."));
      const upload = h("button", { class: "btn scan-upload" }, "Upload and scan");
      const notNow = h("button", { class: "link-btn scan-not-now" }, "Not now");
      upload.addEventListener("click", () => void uploadEntry(e));
      notNow.addEventListener("click", () => {
        e.state = { kind: "skipped" };
        changed(e);
      });
      return h(
        "article",
        { class: "scan-result unseen" },
        head,
        h("p", {}, "VirusTotal hasn't seen this file. Uploading shares it with security companies. Only upload what you'd be fine with others seeing."),
        h("div", { class: "row-buttons" }, upload, notNow),
      );
    }
    const v = verdictOf(a);
    const parts: Node[] = [head, h("p", { class: "scan-verdict" }, h("strong", { class: "scan-word" }, v.word), " ", h("span", { class: "scan-count" }, v.count))];
    if (v.advice) parts.push(h("p", { class: "scan-advice" }, v.advice));
    if (a.names.length) {
      parts.push(h("p", { class: "scan-names muted" }, ...a.names.map(([engine, name], i) => h("span", {}, `${i ? " " : ""}${name} · ${engine}`)), ...(a.more ? [h("span", {}, ` +${a.more} more`)] : [])));
    }
    const report = h("button", { class: "link-btn scan-report" }, "Full report ↗");
    report.addEventListener("click", () => act.openReport(a.link));
    parts.push(h("p", { class: "scan-foot muted" }, ...(a.lastMs ? [`Last scanned ${day(a.lastMs)} · `] : []), report));
    return h("article", { class: `scan-result ${v.tone}` }, ...parts);
  };

  /// What a result's state says, for the screen reader.
  const sayOf = (e: Entry): string => {
    const s = e.state;
    if (s.kind === "checking") return `${e.name}: checking`;
    if (s.kind === "error") return `${e.name}: ${s.why}`;
    if (s.kind === "skipped") return `${e.name}: not uploaded`;
    if (s.kind === "stopped") return `${e.name}: stopped`;
    const a = s.answer;
    if (a.state === "scanning") return `${e.name}: scanning`;
    if (a.state === "notSeen") return `${e.name}: VirusTotal hasn't seen this file`;
    const v = verdictOf(a);
    return `${e.name}: ${v.word}. ${v.count}`;
  };
  const changed = (e: Entry) => {
    draw();
    said.textContent = sayOf(e);
  };

  /// Adds a result at the top and asks for it, following a scan until it's done.
  const run = async (e: Entry, ask: () => Promise<VtAnswer>) => {
    if (!entries.includes(e)) {
      entries.unshift(e);
      entries.splice(KEEP);
    }
    const mine = (e.run = (e.run ?? 0) + 1);
    // Stopped, or asked again, since: what this run hears is no longer wanted.
    const stale = () => e.run !== mine || !entries.includes(e);
    e.state = { kind: "checking" };
    changed(e);
    try {
      let answer = await ask();
      if (stale()) return;
      if (answer.state === "scanning") e.since ??= Date.now();
      e.state = { kind: "answer", answer };
      changed(e);
      while (answer.state === "scanning") {
        const { analysis, then } = answer;
        await new Promise((r) => window.setTimeout(r, FOLLOW_MS));
        if (stale()) return;
        answer = await act.follow(analysis, then);
        if (stale()) return;
        e.state = { kind: "answer", answer };
        changed(e);
      }
      e.since = undefined;
    } catch (err) {
      if (stale()) return;
      e.since = undefined;
      e.state = { kind: "error", why: String(err) };
      changed(e);
    }
  };

  const uploadEntry = (e: Entry) => run(e, () => act.upload(e.path!));

  const view: ScanView = {
    check: (text) => {
      if (!text.trim()) return;
      void run({ name: nameOf(text, false), state: { kind: "checking" } }, () => act.check(text));
    },
    file: (path) => void run({ name: nameOf(path, true), path, state: { kind: "checking" } }, () => act.checkFile(path)),
    refreshKey: async () => {
      if (await act.hasKey()) {
        root.replaceChildren(h("div", { class: "scan" }, drop, h("div", { class: "scan-ask" }, query, checkButton), list, said));
        draw();
        return;
      }
      const settings = h("button", { class: "btn primary scan-settings" }, "Open Kairo's settings");
      settings.addEventListener("click", () => act.openSettings());
      root.replaceChildren(
        h(
          "div",
          { class: "scan scan-no-key" },
          h("p", {}, "Add your VirusTotal key in Kairo's settings to check files and links."),
          h("p", { class: "muted" }, "It's free: virustotal.com → your profile → API key."),
          settings,
        ),
      );
    },
  };

  checkButton.addEventListener("click", () => {
    view.check(query.value);
    query.value = "";
  });
  query.addEventListener("keydown", (e) => {
    if (e.key === "Enter") checkButton.click();
  });
  choose.addEventListener("click", async () => {
    const path = await act.pickFile();
    if (path) view.file(path);
  });

  await view.refreshKey();
  return view;
}
