import { createExecutionContext, createScheduledController, waitOnExecutionContext } from "cloudflare:test";
import { env, exports } from "cloudflare:workers";
import { beforeEach, describe, expect, it } from "vitest";
import worker from "../src/index";

// Every test starts from an empty relay.
beforeEach(async () => {
  await env.DB.batch(["groups", "files", "pairs"].map((t) => env.DB.prepare(`DELETE FROM ${t}`)));
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

  it("refuses files over 2 MB", async () => {
    await makeGroup();
    expect((await call("PUT", file("big"), new Uint8Array(2 * 1024 * 1024 + 1))).status).toBe(413);
    expect((await call("PUT", file("ok"), new Uint8Array(2 * 1024 * 1024))).status).toBe(200);
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
