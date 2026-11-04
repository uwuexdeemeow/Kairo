import { invoke } from "@tauri-apps/api/core";

/// Kairo's VirusTotal commands (src-tauri/src/virustotal.rs, scan_window.rs).

export interface VtVerdict {
  malicious: number;
  suspicious: number;
  /** The engines that gave an answer. */
  total: number;
  /** Up to three detections: [engine, what it called it], malicious first. */
  names: [string, string][];
  /** Detections beyond those three. */
  more: number;
  lastMs: number | null;
  /** The full report on virustotal.com. */
  link: string;
  /** The file's best-known name, or the link. */
  title: string | null;
}

export type VtAnswer =
  | ({ state: "verdict" } & VtVerdict)
  /** A file VirusTotal hasn't seen: uploading is up to the user. */
  | { state: "notSeen"; sha256: string; size: number; tooBig: boolean }
  /** Being scanned; followed with `then` once done. */
  | { state: "scanning"; analysis: string; then: string };

export const vt = {
  hasKey: () => invoke<boolean>("vt_has_key"),
  setKey: (key: string) => invoke<void>("set_vt_key", { key }),
  testKey: () => invoke<void>("test_vt_key"),
  checkFile: (path: string) => invoke<VtAnswer>("vt_check_file", { path }),
  check: (text: string) => invoke<VtAnswer>("vt_check", { text }),
  upload: (path: string) => invoke<VtAnswer>("vt_upload", { path }),
  follow: (analysis: string, then: string) => invoke<VtAnswer>("vt_follow", { analysis, then }),
  pickFile: () => invoke<string | null>("vt_pick_file"),
  openReport: (link: string) => invoke<void>("vt_open_report", { link }),
  /** What the scan window was opened to look up, once. */
  take: () => invoke<string | null>("scan_take"),
  open: (query?: string) => invoke<void>("open_scan", { query }),
};
