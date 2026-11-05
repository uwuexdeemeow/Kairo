import { glyph, type Glyph } from "./glyphs";
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
  rescan(then: string): Promise<VtAnswer>;
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

/// A scan still going is asked about this often.
export const FOLLOW_MS = 20_000;
/// The free key allows this many requests a minute; more wait their turn rather than fail.
export const PER_MINUTE = 4;
const MINUTE = 60_000;
/// The results kept on screen.
const KEEP = 10;
/// A scan going on longer than this is said to be taking a while.
const SLOW_MS = 90_000;

type State =
  | { kind: "checking" }
  /** Its turn under VirusTotal's free limit comes at `until`. */
  | { kind: "waiting"; until: number }
  | { kind: "answer"; answer: VtAnswer }
  | { kind: "error"; why: string }
  | { kind: "skipped" }
  /** No longer followed; VirusTotal carries on, and it can be asked again. */
  | { kind: "stopped"; analysis: string; then: string };

type Entry = {
  /** What it was asked as, so asking again moves it up rather than adding another. */
  key: string;
  name: string;
  /** A hash, which VirusTotal can name better once it answers. */
  hash?: boolean;
  /** The file, for an upload. */
  path?: string;
  /** When VirusTotal started scanning it, for the time shown while it does. */
  since?: number;
  /** Counts each run, so one that was stopped or replaced drops what it hears afterwards. */
  run?: number;
  /** The last request made for it, to try again after an error. */
  ask?: () => Promise<VtAnswer>;
  state: State;
};

const isHash = (t: string) => /^[0-9a-f]{32,64}$/i.test(t);

/// What a result is called: a file's name, a link's host and path, or a hash cut short.
export function nameOf(text: string, isFile: boolean): string {
  if (isFile) return text.split(/[\\/]/).pop() || text;
  const t = text.trim();
  if (isHash(t)) return `${t.slice(0, 12)}…`;
  try {
    const url = new URL(/^https?:\/\//i.test(t) ? t : `https://${t}`);
    const path = url.pathname === "/" ? "" : url.pathname.replace(/\/$/, "");
    return `${url.host}${path}${url.search}` || t;
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

/// Minutes and seconds, as a scan's time so far or a wait's time left.
export const clock = (ms: number) => {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

/// What a scan going on says: how long so far, and that it's taking a while past a minute and a half.
const scanningText = (since: number, now: number) =>
  now - since >= SLOW_MS ? `Taking longer than usual. Kairo keeps checking. ${clock(now - since)}` : `Scanning on VirusTotal… ${clock(now - since)}`;
const waitingText = (until: number, now: number) => `Waiting its turn under VirusTotal's free limit… ${clock(until - now + 999)}`;

/// The errors that mean the key, which only settings can fix.
const aboutKey = (why: string) => /settings/i.test(why);
/// VirusTotal's answer when the free limit was reached anyway (src-tauri/src/virustotal.rs).
const limited = (why: string) => /free limit/i.test(why);

const day = (ms: number) => new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
const sleep = (ms: number) => new Promise((r) => window.setTimeout(r, ms));

let ids = 0;

/// Each tone's stamp: a tick for clean or a lone engine, a cross for malicious, a mark for
/// suspicious or an error, a dash when there's nothing to say.
const SEAL: Record<string, Glyph> = { clean: "seal-clean", fine: "seal-clean", bad: "seal-bad", odd: "seal-odd", error: "seal-odd", none: "seal-none" };

/// A verdict's line: its stamp, its word, and the count or reason behind it.
const verdictLine = (tone: string, word: string, detail: string) =>
  h("div", { class: "scan-verdict" }, glyph(SEAL[tone]), h("p", {}, h("strong", { class: "scan-word" }, word), " ", h("span", { class: "scan-count" }, detail)));

export async function mountScan(root: HTMLElement, act: ScanActions): Promise<ScanView> {
  const entries: Entry[] = [];
  const list = h("div", { class: "scan-results" });
  // Screen readers hear the result that just changed, once, rather than the whole list again.
  const said = h("p", { class: "sr-only", "aria-live": "polite" });
  const title = h("h1", { class: "sr-only" }, "Check a file or link with VirusTotal");
  const query = h("input", { class: "filter scan-query", type: "text", placeholder: "Paste a link or a hash", "aria-label": "A link or a hash", autocomplete: "off", spellcheck: "false" }) as HTMLInputElement;
  const checkButton = h("button", { class: "btn primary scan-check" }, "Check");
  const choose = h("button", { class: "btn scan-choose" }, "Choose file…");
  const drop = h("div", { class: "scan-drop" }, h("p", {}, "Drop a file here"), h("p", { class: "muted" }, "Only its fingerprint is sent, unless you choose to upload it."), choose);

  // ---------- VirusTotal's free limit ----------

  /// When the last requests went, newest last; one more waits until the oldest is a minute old.
  const sent: number[] = [];
  const turn = async (e: Entry, mine: number) => {
    for (;;) {
      const now = Date.now();
      while (sent.length && now - sent[0] >= MINUTE) sent.shift();
      if (sent.length < PER_MINUTE) break;
      const until = sent[0] + MINUTE;
      if (e.state.kind !== "waiting" || e.state.until !== until) {
        e.state = { kind: "waiting", until };
        changed(e);
      }
      await sleep(until - now);
      if (e.run !== mine) return false;
    }
    sent.push(Date.now());
    return true;
  };

  // ---------- drawing ----------

  const draw = () => {
    list.replaceChildren(...entries.map(drawEntry));
    // The newest answer is the figure, though results still being worked on may sit above it.
    list.querySelector(".scan-verdict")?.closest(".scan-result")?.classList.add("lead");
    // With results to read, the way in steps back to a line.
    list.parentElement?.classList.toggle("has-results", entries.length > 0);
  };
  // Times tick in place, so nothing else on the page is redrawn under the keyboard.
  window.setInterval(() => {
    const now = Date.now();
    for (const el of list.querySelectorAll<HTMLElement>("[data-since]")) el.textContent = scanningText(Number(el.dataset.since), now);
    for (const el of list.querySelectorAll<HTMLElement>("[data-until]")) el.textContent = waitingText(Number(el.dataset.until), now);
  }, 1000);

  const card = (e: Entry, tone: string, ...body: Node[]) => {
    const id = `scan-name-${++ids}`;
    return h(
      "article",
      { class: `scan-result${tone ? ` ${tone}` : ""}`, "aria-labelledby": id },
      h("div", { class: "scan-head" }, h("h2", { class: "scan-name", id, title: e.name }, e.name)),
      ...body,
    );
  };
  const button = (cls: string, text: string, then: () => void) => {
    const b = h("button", { class: `link-btn ${cls}` }, text);
    b.addEventListener("click", then);
    return b;
  };

  const drawEntry = (e: Entry): HTMLElement => {
    const s = e.state;
    if (s.kind === "checking") return card(e, "", h("p", { class: "muted" }, "Checking…"));
    if (s.kind === "waiting") return card(e, "", h("p", { class: "muted scan-time", "data-until": String(s.until) }, waitingText(s.until, Date.now())));
    if (s.kind === "error") {
      const tools: Node[] = [];
      if (e.ask) tools.push(button("scan-retry", "Try again", () => void run(e, e.ask!)));
      if (aboutKey(s.why)) tools.push(button("scan-settings-link", "Open Kairo's settings", () => act.openSettings()));
      // Separated as the verdict's own links are.
      const foot = tools.flatMap((t, i) => (i ? [" · ", t] : [t]));
      return card(e, "error", verdictLine("error", "Couldn't check", s.why), ...(foot.length ? [h("p", { class: "scan-foot" }, ...foot)] : []));
    }
    if (s.kind === "skipped") return card(e, "", h("p", { class: "muted" }, "Not uploaded."));
    if (s.kind === "stopped") {
      return card(e, "", h("p", { class: "muted" }, "Stopped. VirusTotal carries on scanning it."), h("p", { class: "scan-foot" }, button("scan-again", "Check again", () => void run(e, () => act.follow(s.analysis, s.then)))));
    }
    const a = s.answer;
    if (a.state === "scanning") {
      const since = e.since ?? Date.now();
      const stop = button("scan-stop", "Stop", () => {
        e.run = (e.run ?? 0) + 1;
        e.since = undefined;
        e.state = { kind: "stopped", analysis: a.analysis, then: a.then };
        changed(e);
      });
      return card(e, "", h("p", { class: "muted scan-time", "data-since": String(since) }, scanningText(since, Date.now())), h("p", { class: "scan-foot" }, stop));
    }
    if (a.state === "unknown") {
      return card(e, "none", verdictLine("none", "Unknown to VirusTotal", "It hasn't seen anything with this hash, so it can't say either way."));
    }
    if (a.state === "notSeen") {
      if (a.tooBig) return card(e, "", h("p", {}, "VirusTotal hasn't seen this file, and it's bigger than the 32 MB a free key can upload."));
      const upload = h("button", { class: "btn scan-upload" }, "Upload and scan");
      upload.addEventListener("click", () => void run(e, () => act.upload(e.path!)));
      const notNow = button("scan-not-now", "Not now", () => {
        e.state = { kind: "skipped" };
        changed(e);
      });
      return card(
        e,
        "unseen",
        h("p", {}, "VirusTotal hasn't seen this file. Uploading shares it with security companies. Only upload what you'd be fine with others seeing."),
        h("div", { class: "row-buttons" }, upload, notNow),
      );
    }
    const v = verdictOf(a);
    const parts: Node[] = [verdictLine(v.tone, v.word, v.count)];
    if (v.advice) parts.push(h("p", { class: "scan-advice" }, v.advice));
    if (a.names.length) {
      parts.push(h("p", { class: "scan-names muted" }, ...a.names.map(([engine, name], i) => h("span", {}, `${i ? " " : ""}${name} · ${engine}`)), ...(a.more ? [h("span", {}, ` +${a.more} more`)] : [])));
    }
    const report = h("button", { class: "link-btn scan-report" }, "Full report", h("span", { "aria-hidden": "true" }, " ↗"));
    report.addEventListener("click", () => act.openReport(a.link));
    const again = button("scan-rescan", "Scan again", () => void run(e, () => act.rescan(a.then)));
    parts.push(h("p", { class: "scan-foot muted" }, ...(a.lastMs ? [`Last scanned ${day(a.lastMs)} · `] : []), report, " · ", again));
    return card(e, v.tone, ...parts);
  };

  /// What a result's state says, for the screen reader.
  const sayOf = (e: Entry): string => {
    const s = e.state;
    if (s.kind === "checking") return `${e.name}: checking`;
    if (s.kind === "waiting") return `${e.name}: waiting its turn under VirusTotal's free limit`;
    if (s.kind === "error") return `${e.name}: couldn't check. ${s.why}`;
    if (s.kind === "skipped") return `${e.name}: not uploaded`;
    if (s.kind === "stopped") return `${e.name}: stopped`;
    const a = s.answer;
    if (a.state === "scanning") return `${e.name}: scanning`;
    if (a.state === "unknown") return `${e.name}: unknown to VirusTotal`;
    if (a.state === "notSeen") return `${e.name}: VirusTotal hasn't seen this file`;
    const v = verdictOf(a);
    return `${e.name}: ${v.word}. ${v.count}`;
  };
  const changed = (e: Entry) => {
    if (!busy(e)) trim();
    draw();
    // Cleared first, so the same words said again (the same thing checked twice) are heard again.
    said.textContent = "";
    said.textContent = sayOf(e);
  };

  // ---------- asking ----------

  /// Whether it's still being worked on, so it isn't pushed off the list mid-scan.
  const busy = (e: Entry) => e.state.kind === "checking" || e.state.kind === "waiting" || (e.state.kind === "answer" && e.state.answer.state === "scanning");

  /// Puts a result at the top, the one already there for the same thing if any, and keeps ten,
  /// dropping the oldest that's done before any still going.
  const place = (e: Entry): Entry => {
    const was = entries.find((x) => x.key === e.key) ?? e;
    const at = entries.indexOf(was);
    if (at >= 0) entries.splice(at, 1);
    entries.unshift(was);
    trim();
    return was;
  };
  /// Back to ten, oldest done first; one still going stays until it's done.
  const trim = () => {
    for (let i = entries.length - 1; entries.length > KEEP && i > 0; i--) if (!busy(entries[i])) entries.splice(i, 1);
  };

  /// Asks for a result, waiting its turn under the free limit, and follows a scan until it's done.
  const run = async (e: Entry, ask: () => Promise<VtAnswer>) => {
    if (!entries.includes(e)) place(e);
    const mine = (e.run = (e.run ?? 0) + 1);
    // Stopped, asked again or dropped since: what this run hears is no longer wanted.
    const stale = () => e.run !== mine || !entries.includes(e);
    e.ask = ask;
    e.state = { kind: "checking" };
    changed(e);
    const request = async (call: () => Promise<VtAnswer>): Promise<VtAnswer | null> => {
      for (let tries = 0; ; tries++) {
        if (!(await turn(e, mine)) || stale()) return null;
        if (e.state.kind === "waiting") {
          e.state = { kind: "checking" };
          changed(e);
        }
        try {
          return await call();
        } catch (err) {
          // Over the limit after all (another app on the same key, say): wait a minute, once.
          if (tries || !limited(String(err))) throw err;
          sent.push(...Array(PER_MINUTE).fill(Date.now()));
        }
      }
    };
    try {
      const first = await request(ask);
      if (!first || stale()) return;
      let answer: VtAnswer = first;
      if (answer.state === "scanning") e.since ??= Date.now();
      if (answer.state === "verdict" && e.hash && answer.title) e.name = answer.title;
      e.state = { kind: "answer", answer };
      changed(e);
      while (answer.state === "scanning") {
        const { analysis, then } = answer;
        await sleep(FOLLOW_MS);
        if (stale()) return;
        answer = (await request(() => act.follow(analysis, then))) ?? answer;
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

  const view: ScanView = {
    check: (text) => {
      const t = text.trim();
      if (!t) return;
      const e = place({ key: `q:${isHash(t) ? t.toLowerCase() : t}`, name: nameOf(t, false), hash: isHash(t), state: { kind: "checking" } });
      void run(e, () => act.check(t));
    },
    file: (path) => {
      const e = place({ key: `f:${path}`, name: nameOf(path, true), path, state: { kind: "checking" } });
      void run(e, () => act.checkFile(path));
    },
    refreshKey: async () => {
      const key = await act.hasKey();
      // Drawn again only when that changes: redrawing takes the keyboard away from the field.
      if (key === hasKey) return;
      hasKey = key;
      if (key) {
        root.replaceChildren(h("div", { class: "scan" }, title, drop, h("div", { class: "scan-ask" }, query, checkButton), list, said));
        draw();
        return;
      }
      const settings = h("button", { class: "btn primary scan-settings" }, "Open Kairo's settings");
      settings.addEventListener("click", () => act.openSettings());
      root.replaceChildren(
        h(
          "div",
          { class: "scan scan-no-key" },
          title,
          h("p", {}, "Add your VirusTotal key in Kairo's settings to check files and links."),
          h("p", { class: "muted" }, "It's free: virustotal.com → your profile → API key."),
          settings,
        ),
      );
    },
  };
  let hasKey: boolean | null = null;

  checkButton.addEventListener("click", () => {
    if (!query.value.trim()) {
      // Nothing to check: the field says so, rather than the button doing nothing.
      query.setAttribute("aria-invalid", "true");
      said.textContent = "Paste a link or a hash first.";
      query.focus();
      return;
    }
    query.removeAttribute("aria-invalid");
    view.check(query.value);
    query.value = "";
  });
  query.addEventListener("input", () => query.removeAttribute("aria-invalid"));
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
