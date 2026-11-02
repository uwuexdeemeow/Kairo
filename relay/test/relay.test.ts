import { createExecutionContext, createScheduledController, waitOnExecutionContext } from "cloudflare:test";
import { env, exports } from "cloudflare:workers";
import { beforeEach, describe, expect, it } from "vitest";
import worker from "../src/index";

// Every test starts from an empty relay.
beforeEach(async () => {
  await env.DB.batch(["groups", "files", "gone_files", "pairs"].map((t) => env.DB.prepare(`DELETE FROM ${t}`)));
});

const BASE = "https://relay.test";
const GROUP = "0123456789abcdef0123456789abcdef";
const TOKEN = "token-of-the-group";
const COMPUTER = "a1b2c3d4e5f60718";

async function sha256(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

function call(method: string, path: string, body?: BodyInit | object, token: string | null = TOKEN): Promise<Response> {
  const headers: Record<string, string> = {};
  if (token) headers.authorization = `Bearer ${token}`;
  let payload: BodyInit | undefined;
  if (body instanceof Uint8Array || typeof body === "string") payload = body as BodyInit;
  else if (body !== undefined) {
    payload = JSON.stringify(body);
    headers["content-type"] = "application/json";
  }
  return exports.default.fetch(new Request(BASE + path, { method, headers, body: payload }));
}

async function makeGroup(id = GROUP): Promise<void> {
  const res = await call("POST", "/v1/groups", { id, tokenHash: await sha256(TOKEN) }, null);
  expect(res.status).toBe(201);
}

const file = (name: string) => `/v1/groups/${GROUP}/files/${COMPUTER}/${name}`;

describe("groups", () => {
  it("needs the token", async () => {
    await makeGroup();
    expect((await call("GET", `/v1/groups/${GROUP}/files`, undefined, null)).status).toBe(401);
    expect((await call("GET", `/v1/groups/${GROUP}/files`, undefined, "wrong")).status).toBe(401);
    expect((await call("GET", `/v1/groups/${"f".repeat(32)}/files`)).status).toBe(404);
    expect((await call("GET", `/v1/groups/${GROUP}/files`)).status).toBe(200);
  });

  it("won't make the same group twice", async () => {
    await makeGroup();
    const again = await call("POST", "/v1/groups", { id: GROUP, tokenHash: await sha256("other") }, null);
    expect(again.status).toBe(409);
    expect((await call("POST", "/v1/groups", { id: "nope", tokenHash: "x" }, null)).status).toBe(400);
  });

  it("stores lists and deletes files", async () => {
    await makeGroup();
    expect((await call("PUT", file("abc"), new Uint8Array([1, 2, 3]))).status).toBe(200);
    const got = await call("GET", file("abc"));
    expect(new Uint8Array(await got.arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]));

    const first = (await (await call("GET", `/v1/groups/${GROUP}/files`)).json()) as { computer: string; name: string; version: string }[];
    expect(first).toEqual([{ computer: COMPUTER, name: "abc", version: "1" }]);
    await call("PUT", file("abc"), new Uint8Array([4]));
    const second = (await (await call("GET", `/v1/groups/${GROUP}/files`)).json()) as { version: string }[];
    expect(second[0].version).toBe("2");

    expect((await call("DELETE", file("abc"))).status).toBe(200);
    expect((await call("GET", file("abc"))).status).toBe(404);
    await call("PUT", file("one"), new Uint8Array([1]));
    await call("PUT", file("two"), new Uint8Array([2]));
    expect((await call("DELETE", `/v1/groups/${GROUP}/computers/${COMPUTER}`)).status).toBe(200);
    expect(await (await call("GET", `/v1/groups/${GROUP}/files`)).json()).toEqual([]);
  });

  it("refuses files over 1.9 MB", async () => {
    await makeGroup();
    expect((await call("PUT", file("big"), new Uint8Array(1_900_001))).status).toBe(413);
    expect((await call("PUT", file("ok"), new Uint8Array(1_900_000))).status).toBe(200);
  });

  it("refuses groups over 50 MB", async () => {
    await makeGroup();
    await env.DB.prepare("UPDATE groups SET bytes = ? WHERE id = ?").bind(50 * 1024 * 1024 - 10, GROUP).run();
    expect((await call("PUT", file("x"), new Uint8Array(11))).status).toBe(507);
    expect((await call("PUT", file("x"), new Uint8Array(10))).status).toBe(200);
    // Rewriting a file counts only the difference.
    expect((await call("PUT", file("x"), new Uint8Array(10))).status).toBe(200);
  });

  it("deleting a group removes its files", async () => {
    await makeGroup();
    await call("PUT", file("abc"), new Uint8Array([1]));
    expect((await call("DELETE", `/v1/groups/${GROUP}`)).status).toBe(200);
    expect((await call("GET", `/v1/groups/${GROUP}/files`)).status).toBe(404);
    const left = await env.DB.prepare("SELECT COUNT(*) AS n FROM files").first<{ n: number }>();
    expect(left?.n).toBe(0);
  });

  it("refuses names that aren't sealed names", async () => {
    await makeGroup();
    expect((await call("PUT", `/v1/groups/${GROUP}/files/${COMPUTER}/..%2Fescape`, new Uint8Array([1]))).status).toBe(400);
    expect((await call("PUT", `/v1/groups/${GROUP}/files/NOT-HEX/abc`, new Uint8Array([1]))).status).toBe(400);
  });
});

describe("listing what changed", () => {
  type Changes = { seq: string; full: boolean; files: { computer: string; name: string; version: string }[]; gone: { computer: string; name: string; version: string }[] };
  const since = async (seq: string | number): Promise<Changes> => (await (await call("GET", `/v1/groups/${GROUP}/files?since=${seq}`)).json()) as Changes;

  it("lists everything from 0, then only what changed", async () => {
    await makeGroup();
    await call("PUT", file("one"), new Uint8Array([1]));
    await call("PUT", file("two"), new Uint8Array([2]));
    const all = await since(0);
    expect(all.full).toBe(true);
    expect(all.files.map((f) => f.name).sort()).toEqual(["one", "two"]);
    expect(all.gone).toEqual([]);

    expect(await since(all.seq)).toEqual({ seq: all.seq, full: false, files: [], gone: [] });
    await call("PUT", file("two"), new Uint8Array([3]));
    const next = await since(all.seq);
    expect(next.files.map((f) => f.name)).toEqual(["two"]);
    expect(Number(next.seq)).toBeGreaterThan(Number(all.seq));
    // Versions go up across the group, so one number says where a computer got to.
    expect(Number(next.files[0].version)).toBe(Number(next.seq));
  });

  it("says what went, until it puts the file back", async () => {
    await makeGroup();
    await call("PUT", file("one"), new Uint8Array([1]));
    await call("PUT", file("two"), new Uint8Array([2]));
    const before = await since(0);
    await call("DELETE", file("one"));
    const after = await since(before.seq);
    expect(after.files).toEqual([]);
    expect(after.gone.map((f) => f.name)).toEqual(["one"]);
    await call("PUT", file("one"), new Uint8Array([9]));
    const back = await since(before.seq);
    expect(back.files.map((f) => f.name)).toEqual(["one"]);
    expect(back.gone).toEqual([]);

    // A computer leaving takes all its files.
    await call("DELETE", `/v1/groups/${GROUP}/computers/${COMPUTER}`);
    expect((await since(back.seq)).gone.map((f) => f.name).sort()).toEqual(["one", "two"]);
  });

  it("starts over from a full list once what went has been forgotten", async () => {
    await makeGroup();
    await call("PUT", file("one"), new Uint8Array([1]));
    const before = await since(0);
    await call("DELETE", file("one"));
    await env.DB.prepare("UPDATE gone_files SET gone_ms = ?").bind(Date.now() - 31 * 24 * 3600 * 1000).run();
    const ctx = createExecutionContext();
    await worker.scheduled(createScheduledController(), env, ctx);
    await waitOnExecutionContext(ctx);
    const later = await since(before.seq);
    expect(later.full).toBe(true);
    expect(later.files).toEqual([]);
  });

  it("keeps the old list for older Kairo", async () => {
    await makeGroup();
    await call("PUT", file("one"), new Uint8Array([1]));
    expect(await (await call("GET", `/v1/groups/${GROUP}/files`)).json()).toEqual([{ computer: COMPUTER, name: "one", version: "1" }]);
    expect((await call("GET", `/v1/groups/${GROUP}/files?since=-1`)).status).toBe(400);
  });

  it("reads only what changed, through an index", async () => {
    const plan = async (sql: string) =>
      JSON.stringify((await env.DB.prepare(`EXPLAIN QUERY PLAN ${sql}`).bind(GROUP, 5).all()).results);
    expect(await plan("SELECT computer, name, version FROM files WHERE group_id = ? AND version > ?")).toContain("files_by_version");
    expect(await plan("SELECT computer, name, version FROM gone_files WHERE group_id = ? AND version > ?")).toContain("gone_by_version");
  });
});

describe("pairing", () => {
  async function open(): Promise<string> {
    const res = await call("POST", "/v1/pair", { a: "bWVzc2FnZS1h" }, null);
    expect(res.status).toBe(200);
    const { slot } = (await res.json()) as { slot: string };
    expect(slot).toMatch(/^[0-9A-HJKMNP-TV-Z]{2}$/);
    return slot;
  }

  it("pair parts are write-once", async () => {
    const slot = await open();
    expect((await call("PUT", `/v1/pair/${slot}/b`, { data: "Yg" }, null)).status).toBe(200);
    expect((await call("PUT", `/v1/pair/${slot}/b`, { data: "Yg" }, null)).status).toBe(409);
    expect((await call("PUT", `/v1/pair/${slot}/invite`, { data: "aQ" }, null)).status).toBe(200);
    expect((await call("PUT", `/v1/pair/${slot}/reply`, { data: "cg" }, null)).status).toBe(200);
    expect((await call("PUT", `/v1/pair/${slot}/other`, { data: "eA" }, null)).status).toBe(400);
    const parts = await (await call("GET", `/v1/pair/${slot}`, undefined, null)).json();
    expect(parts).toEqual({ a: "bWVzc2FnZS1h", b: "Yg", invite: "aQ", reply: "cg" });
  });

  it("shows only the parts sent so far", async () => {
    const slot = await open();
    expect(await (await call("GET", `/v1/pair/${slot}`, undefined, null)).json()).toEqual({ a: "bWVzc2FnZS1h" });
  });

  it("expired slots are gone", async () => {
    const slot = await open();
    await env.DB.prepare("UPDATE pairs SET expires_ms = ? WHERE slot = ?").bind(Date.now() - 1, slot).run();
    expect((await call("GET", `/v1/pair/${slot}`, undefined, null)).status).toBe(404);
    expect((await call("PUT", `/v1/pair/${slot}/b`, { data: "Yg" }, null)).status).toBe(404);
  });

  it("closing frees the slot", async () => {
    const slot = await open();
    expect((await call("DELETE", `/v1/pair/${slot}`, undefined, null)).status).toBe(200);
    expect((await call("GET", `/v1/pair/${slot}`, undefined, null)).status).toBe(404);
  });

  it("parts go in order: an invitation needs an answer, a reply an invitation", async () => {
    const slot = await open();
    expect((await call("PUT", `/v1/pair/${slot}/invite`, { data: "aQ" }, null)).status).toBe(409);
    expect((await call("PUT", `/v1/pair/${slot}/reply`, { data: "cg" }, null)).status).toBe(409);
    expect((await call("PUT", `/v1/pair/${slot}/b`, { data: "Yg" }, null)).status).toBe(200);
    expect((await call("PUT", `/v1/pair/${slot}/reply`, { data: "cg" }, null)).status).toBe(409);
  });

  describe("with secrets", () => {
    const HOST = "secret-of-the-host";
    const JOINER = "secret-of-the-joiner";

    async function openWithSecret(): Promise<string> {
      const res = await call("POST", "/v1/pair", { a: "bWVzc2FnZS1h", hostHash: await sha256(HOST) }, null);
      expect(res.status).toBe(200);
      return ((await res.json()) as { slot: string }).slot;
    }

    it("only the host writes the invitation or closes the slot", async () => {
      const slot = await openWithSecret();
      expect((await call("PUT", `/v1/pair/${slot}/b`, { data: "Yg", joinerHash: await sha256(JOINER) }, null)).status).toBe(200);
      for (const token of [null, "guess", JOINER]) {
        expect((await call("PUT", `/v1/pair/${slot}/invite`, { data: "aQ" }, token)).status).toBe(401);
        expect((await call("DELETE", `/v1/pair/${slot}`, undefined, token)).status).toBe(401);
      }
      expect((await call("PUT", `/v1/pair/${slot}/invite`, { data: "aQ" }, HOST)).status).toBe(200);
      // The secrets never show.
      expect(await (await call("GET", `/v1/pair/${slot}`, undefined, null)).json()).toEqual({ a: "bWVzc2FnZS1h", b: "Yg", invite: "aQ" });
      expect((await call("DELETE", `/v1/pair/${slot}`, undefined, HOST)).status).toBe(200);
    });

    it("only the computer that answered replies", async () => {
      const slot = await openWithSecret();
      expect((await call("PUT", `/v1/pair/${slot}/b`, { data: "Yg", joinerHash: await sha256(JOINER) }, null)).status).toBe(200);
      expect((await call("PUT", `/v1/pair/${slot}/invite`, { data: "aQ" }, HOST)).status).toBe(200);
      for (const token of [null, "guess", HOST]) {
        expect((await call("PUT", `/v1/pair/${slot}/reply`, { data: "cg" }, token)).status).toBe(401);
      }
      expect((await call("PUT", `/v1/pair/${slot}/reply`, { data: "cg" }, JOINER)).status).toBe(200);
    });

    it("an answer without its secret's hash is refused", async () => {
      const slot = await openWithSecret();
      expect((await call("PUT", `/v1/pair/${slot}/b`, { data: "Yg" }, null)).status).toBe(400);
      expect((await call("PUT", `/v1/pair/${slot}/b`, { data: "Yg", joinerHash: "not-a-hash" }, null)).status).toBe(400);
    });
  });

  it("refuses big pairing messages", async () => {
    expect((await call("POST", "/v1/pair", { a: "x".repeat(5000) }, null)).status).toBe(413);
  });
});

describe("the daily clean-up", () => {
  it("clears expired slots and idle groups", async () => {
    await makeGroup();
    await call("PUT", file("abc"), new Uint8Array([1]));
    await env.DB.prepare("UPDATE groups SET used_ms = ?").bind(Date.now() - 181 * 24 * 3600 * 1000).run();
    await env.DB.prepare("INSERT INTO pairs (slot, a, expires_ms) VALUES ('ZZ', 'a', ?)").bind(Date.now() - 1).run();
    const ctx = createExecutionContext();
    await worker.scheduled(createScheduledController(), env, ctx);
    await waitOnExecutionContext(ctx);
    const count = async (table: string) => (await env.DB.prepare(`SELECT COUNT(*) AS n FROM ${table}`).first<{ n: number }>())?.n;
    expect(await count("groups")).toBe(0);
    expect(await count("files")).toBe(0);
    expect(await count("pairs")).toBe(0);
  });
});
