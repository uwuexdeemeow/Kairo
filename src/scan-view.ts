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

type Entry = {
  name: string;
  /** The file, for an upload. */
  path?: string;
  state: { kind: "checking" } | { kind: "answer"; answer: VtAnswer } | { kind: "error"; why: string } | { kind: "skipped" };
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

/// How bad a verdict is: any malicious is bad, only suspicious odd, none clean.
function tone(v: VtVerdict): "bad" | "odd" | "clean" {
  return v.malicious ? "bad" : v.suspicious ? "odd" : "clean";
}

const day = (ms: number) => new Date(ms).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });

export async function mountScan(root: HTMLElement, act: ScanActions): Promise<ScanView> {
  const entries: Entry[] = [];
  const list = h("div", { class: "scan-results", "aria-live": "polite" });
  const query = h("input", { class: "filter scan-query", type: "text", placeholder: "Paste a link or a hash", "aria-label": "A link or a hash", autocomplete: "off", spellcheck: "false" }) as HTMLInputElement;
  const checkButton = h("button", { class: "btn primary scan-check" }, "Check");
  const choose = h("button", { class: "btn scan-choose" }, "Choose file…");
  const drop = h("div", { class: "scan-drop" }, h("p", {}, "Drop a file here"), h("p", { class: "muted" }, "Only its fingerprint is sent, unless you choose to upload it."), choose);

  const draw = () => {
    list.replaceChildren(...entries.map(drawEntry));
  };

  const drawEntry = (e: Entry): HTMLElement => {
    const head = h("div", { class: "scan-head" }, h("strong", { class: "scan-name" }, e.name));
    const s = e.state;
    if (s.kind === "checking") return h("article", { class: "scan-result" }, head, h("p", { class: "muted" }, "Checking…"));
    if (s.kind === "error") return h("article", { class: "scan-result error" }, head, h("p", {}, s.why));
    if (s.kind === "skipped") return h("article", { class: "scan-result" }, head, h("p", { class: "muted" }, "Not uploaded."));
    const a = s.answer;
    if (a.state === "scanning") return h("article", { class: "scan-result" }, head, h("p", { class: "muted" }, "◌ Scanning… usually under a minute"));
    if (a.state === "notSeen") {
      if (a.tooBig) return h("article", { class: "scan-result" }, head, h("p", {}, "VirusTotal hasn't seen this file. Too big to upload with a free key."));
      const upload = h("button", { class: "btn scan-upload" }, "Upload and scan");
      const notNow = h("button", { class: "link-btn scan-not-now" }, "Not now");
      upload.addEventListener("click", () => void uploadEntry(e));
      notNow.addEventListener("click", () => {
        e.state = { kind: "skipped" };
        draw();
      });
      return h(
        "article",
        { class: "scan-result unseen" },
        head,
        h("p", {}, "VirusTotal hasn't seen this file. Uploading shares it with security companies. Only upload what you'd be fine with others seeing."),
        h("div", { class: "row-buttons" }, upload, notNow),
      );
    }
    const flagged = a.malicious + a.suspicious;
    const parts: Node[] = [head, h("p", { class: "scan-verdict" }, `● ${flagged} of ${a.total} engines flag it`)];
    if (a.names.length) {
      parts.push(h("p", { class: "scan-names muted" }, ...a.names.map(([engine, name], i) => h("span", {}, `${i ? " " : ""}${name} · ${engine}`)), ...(a.more ? [h("span", {}, ` +${a.more} more`)] : [])));
    }
    const report = h("button", { class: "link-btn scan-report" }, "Full report ↗");
    report.addEventListener("click", () => act.openReport(a.link));
    parts.push(h("p", { class: "scan-foot muted" }, ...(a.lastMs ? [`Last scanned ${day(a.lastMs)} · `] : []), report));
    return h("article", { class: `scan-result ${tone(a)}` }, ...parts);
  };

  /// Adds a result at the top and asks for it, following a scan until it's done.
  const run = async (e: Entry, ask: () => Promise<VtAnswer>) => {
    if (!entries.includes(e)) {
      entries.unshift(e);
      entries.splice(KEEP);
    }
    e.state = { kind: "checking" };
    draw();
    try {
      let answer = await ask();
      e.state = { kind: "answer", answer };
      draw();
      while (answer.state === "scanning" && entries.includes(e)) {
        const { analysis, then } = answer;
        await new Promise((r) => window.setTimeout(r, FOLLOW_MS));
        answer = await act.follow(analysis, then);
        e.state = { kind: "answer", answer };
        draw();
      }
    } catch (err) {
      e.state = { kind: "error", why: String(err) };
      draw();
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
        root.replaceChildren(h("div", { class: "scan" }, drop, h("div", { class: "scan-ask" }, query, checkButton), list));
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
