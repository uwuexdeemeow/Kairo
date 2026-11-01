// Kairo's relay. Linked computers keep their files here, sealed with a key the relay never sees,
// under names it can't read; it checks a token's hash to know who may read and write. It also
// holds the few messages two computers swap while linking with a code. See
// docs/superpowers/specs/2026-11-02-pairing-relay-design.md.

export interface Env {
  DB: D1Database;
}

const MB = 1024 * 1024;
const FILE_MAX = 2 * MB;
const GROUP_MAX = 50 * MB;
const PAIR_MAX = 4096;
const SLOT_MS = 10 * 60 * 1000;
const IDLE_MS = 180 * 24 * 60 * 60 * 1000;
const DAY_MS = 24 * 60 * 60 * 1000;
const ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const PARTS = new Set(["b", "invite", "reply"]);

const GROUP_ID = /^[0-9a-f]{32}$/;
const HASH = /^[0-9a-f]{64}$/;
const COMPUTER = /^[0-9a-f]{1,64}$/;
const NAME = /^[A-Za-z0-9_-]{1,512}$/;
const SLOT = /^[0-9A-HJKMNP-TV-Z]{2}$/;
const BASE64 = /^[A-Za-z0-9+/_=-]*$/;

const status = (code: number, message = "") => new Response(message, { status: code });
const json = (value: unknown) => Response.json(value);

async function sha256(text: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

/** A small JSON body, or null when it's too big or not JSON. */
async function smallJson(request: Request): Promise<Record<string, unknown> | "too big" | null> {
  const text = await request.text();
  if (text.length > PAIR_MAX) return "too big";
  try {
    const value = JSON.parse(text);
    return value && typeof value === "object" ? value : null;
  } catch {
    return null;
  }
}

/** 404 when there's no such group, 401 when the token doesn't match, null when it does. */
async function authorise(request: Request, env: Env, group: string): Promise<Response | null> {
  const row = await env.DB.prepare("SELECT token_hash FROM groups WHERE id = ?").bind(group).first<{ token_hash: string }>();
  if (!row) return status(404, "no such group");
  const token = request.headers.get("authorization")?.match(/^Bearer (.+)$/)?.[1];
  if (!token || (await sha256(token)) !== row.token_hash) return status(401, "wrong token");
  // Noted at most once a day, so an active group isn't cleared as idle.
  const now = Date.now();
  await env.DB.prepare("UPDATE groups SET used_ms = ? WHERE id = ? AND used_ms < ?").bind(now, group, now - DAY_MS).run();
  return null;
}

async function createGroup(request: Request, env: Env): Promise<Response> {
  const body = await smallJson(request);
  if (body === "too big") return status(413);
  const id = body?.id, tokenHash = body?.tokenHash;
  if (typeof id !== "string" || !GROUP_ID.test(id) || typeof tokenHash !== "string" || !HASH.test(tokenHash)) return status(400);
  const now = Date.now();
  const made = await env.DB.prepare("INSERT OR IGNORE INTO groups (id, token_hash, created_ms, used_ms) VALUES (?, ?, ?, ?)")
    .bind(id, tokenHash, now, now)
    .run();
  return made.meta.changes ? status(201) : status(409, "that group exists");
}

async function putFile(request: Request, env: Env, group: string, computer: string, name: string): Promise<Response> {
  const data = new Uint8Array(await request.arrayBuffer());
  if (data.byteLength > FILE_MAX) return status(413, "file too big");
  const [old, usage] = await Promise.all([
    env.DB.prepare("SELECT size, version FROM files WHERE group_id = ? AND computer = ? AND name = ?")
      .bind(group, computer, name)
      .first<{ size: number; version: number }>(),
    env.DB.prepare("SELECT bytes FROM groups WHERE id = ?").bind(group).first<{ bytes: number }>(),
  ]);
  const grows = data.byteLength - (old?.size ?? 0);
  if ((usage?.bytes ?? 0) + grows > GROUP_MAX) return status(507, "group full");
  const version = (old?.version ?? 0) + 1;
  await env.DB.batch([
    env.DB.prepare("INSERT OR REPLACE INTO files (group_id, computer, name, version, data, size) VALUES (?, ?, ?, ?, ?, ?)").bind(
      group,
      computer,
      name,
      version,
      data,
      data.byteLength,
    ),
    env.DB.prepare("UPDATE groups SET bytes = bytes + ? WHERE id = ?").bind(grows, group),
  ]);
  return json({ version: String(version) });
}

async function deleteFiles(env: Env, group: string, computer: string, name?: string): Promise<Response> {
  const where = name === undefined ? "group_id = ? AND computer = ?" : "group_id = ? AND computer = ? AND name = ?";
  const binds = name === undefined ? [group, computer] : [group, computer, name];
  const freed = await env.DB.prepare(`SELECT COALESCE(SUM(size), 0) AS n FROM files WHERE ${where}`).bind(...binds).first<{ n: number }>();
  await env.DB.batch([
    env.DB.prepare(`DELETE FROM files WHERE ${where}`).bind(...binds),
    env.DB.prepare("UPDATE groups SET bytes = MAX(0, bytes - ?) WHERE id = ?").bind(freed?.n ?? 0, group),
  ]);
  return status(200);
}

async function groups(request: Request, env: Env, rest: string[]): Promise<Response> {
  const [group, kind, computer, name] = rest;
  if (!GROUP_ID.test(group ?? "")) return status(400);
  const denied = await authorise(request, env, group);
  if (denied) return denied;
  const method = request.method;

  if (kind === undefined && method === "DELETE") {
    await env.DB.batch([env.DB.prepare("DELETE FROM files WHERE group_id = ?").bind(group), env.DB.prepare("DELETE FROM groups WHERE id = ?").bind(group)]);
    return status(200);
  }
  if (kind === "files" && computer === undefined && method === "GET") {
    const { results } = await env.DB.prepare("SELECT computer, name, version FROM files WHERE group_id = ? ORDER BY computer, name")
      .bind(group)
      .all<{ computer: string; name: string; version: number }>();
    return json(results.map((r) => ({ computer: r.computer, name: r.name, version: String(r.version) })));
  }
  if (kind === "computers" && computer !== undefined && name === undefined && method === "DELETE") {
    if (!COMPUTER.test(computer)) return status(400);
    return deleteFiles(env, group, computer);
  }
  if (kind === "files" && computer !== undefined && name !== undefined && rest.length === 4) {
    if (!COMPUTER.test(computer) || !NAME.test(name)) return status(400);
    if (method === "PUT") return putFile(request, env, group, computer, name);
    if (method === "DELETE") return deleteFiles(env, group, computer, name);
    if (method === "GET") {
      const row = await env.DB.prepare("SELECT data FROM files WHERE group_id = ? AND computer = ? AND name = ?")
        .bind(group, computer, name)
        .first<{ data: ArrayBuffer | number[] }>();
      if (!row) return status(404, "no such file");
      return new Response(new Uint8Array(row.data), { headers: { "content-type": "application/octet-stream" } });
    }
  }
  return status(404);
}

async function openSlot(request: Request, env: Env): Promise<Response> {
  const body = await smallJson(request);
  if (body === "too big") return status(413);
  const a = body?.a;
  if (typeof a !== "string" || !a || !BASE64.test(a)) return status(400);
  const now = Date.now();
  await env.DB.prepare("DELETE FROM pairs WHERE expires_ms < ?").bind(now).run();
  // Random slots first; when those are taken, any free one.
  const candidates: string[] = [];
  for (let i = 0; i < 16; i++) {
    const bytes = crypto.getRandomValues(new Uint8Array(2));
    candidates.push(ALPHABET[bytes[0] % 32] + ALPHABET[bytes[1] % 32]);
  }
  for (const first of ALPHABET) for (const second of ALPHABET) candidates.push(first + second);
  for (const slot of candidates) {
    const made = await env.DB.prepare("INSERT OR IGNORE INTO pairs (slot, a, expires_ms) VALUES (?, ?, ?)").bind(slot, a, now + SLOT_MS).run();
    if (made.meta.changes) return json({ slot });
  }
  return status(503, "every slot is busy");
}

async function pair(request: Request, env: Env, rest: string[]): Promise<Response> {
  const [slot, part] = rest;
  if (slot === undefined && request.method === "POST") return openSlot(request, env);
  if (!SLOT.test(slot ?? "") || rest.length > 2) return status(400);
  const now = Date.now();

  if (part === undefined && request.method === "GET") {
    const row = await env.DB.prepare("SELECT a, b, invite, reply FROM pairs WHERE slot = ? AND expires_ms >= ?")
      .bind(slot, now)
      .first<Record<string, string | null>>();
    if (!row) return status(404, "no such slot");
    return json(Object.fromEntries(Object.entries(row).filter(([, v]) => v !== null)));
  }
  if (part === undefined && request.method === "DELETE") {
    await env.DB.prepare("DELETE FROM pairs WHERE slot = ?").bind(slot).run();
    return status(200);
  }
  if (part !== undefined && request.method === "PUT") {
    if (!PARTS.has(part)) return status(400);
    const body = await smallJson(request);
    if (body === "too big") return status(413);
    const data = body?.data;
    if (typeof data !== "string" || !data || !BASE64.test(data)) return status(400);
    // Each part once: a second answer to a code is refused, so a code allows one guess.
    const written = await env.DB.prepare(`UPDATE pairs SET ${part} = ? WHERE slot = ? AND ${part} IS NULL AND expires_ms >= ?`)
      .bind(data, slot, now)
      .run();
    if (written.meta.changes) return status(200);
    const exists = await env.DB.prepare("SELECT 1 FROM pairs WHERE slot = ? AND expires_ms >= ?").bind(slot, now).first();
    return exists ? status(409, "already sent") : status(404, "no such slot");
  }
  return status(404);
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const parts = new URL(request.url).pathname.split("/").filter(Boolean).map(decodeURIComponent);
    if (parts[0] !== "v1") return status(404);
    if (parts[1] === "groups" && parts.length === 2 && request.method === "POST") return createGroup(request, env);
    if (parts[1] === "groups") return groups(request, env, parts.slice(2));
    if (parts[1] === "pair") return pair(request, env, parts.slice(2));
    return status(404);
  },

  async scheduled(_controller: ScheduledController, env: Env, _ctx: ExecutionContext): Promise<void> {
    const now = Date.now();
    const idle = now - IDLE_MS;
    await env.DB.batch([
      env.DB.prepare("DELETE FROM pairs WHERE expires_ms < ?").bind(now),
      env.DB.prepare("DELETE FROM files WHERE group_id IN (SELECT id FROM groups WHERE used_ms < ?)").bind(idle),
      env.DB.prepare("DELETE FROM groups WHERE used_ms < ?").bind(idle),
    ]);
  },
} satisfies ExportedHandler<Env>;
