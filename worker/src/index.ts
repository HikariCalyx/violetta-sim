import { USERBOT_INBOX_NAME, userIndexName, type Env } from './env';
import { dispatch } from './gating';
import { ChatRegistry } from './registry';
import { USERBOT_MUTE_SECONDS, VerificationSession } from './session';
import type { TgUpdate } from './telegram';
import { escapeHtml } from './telegram';
import { UserbotInbox } from './userbot';
import { UserIndex } from './user-index';

// Durable Object classes have to be exported from the Worker's entry point.
export { ChatRegistry, UserIndex, UserbotInbox, VerificationSession };

const UUID = '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}';
const SESSION_API = new RegExp(`^/api/session/(${UUID})/(state|pick)$`);
const SESSION_ID = new RegExp(`^${UUID}$`);
const USERBOT_SESSION_API = new RegExp(`^/api/userbot/session/(${UUID})/(sent|abort)$`);
const USERBOT_ACTION = new RegExp(`^/api/userbot/actions/(${UUID})$`);

const json = (data: unknown, status = 200): Response =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' },
  });

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);

    // Everything that exists in `dist/` (the game, its wasm and js) is served by the asset
    // binding before the Worker runs; only the paths below reach this code.
    if (url.pathname === '/telegram/webhook') return handleWebhook(request, env, ctx);
    if (url.pathname === '/telegram' && request.method === 'GET') return serveGame(request, env, url);
    if (url.pathname === '/api/health') return json(health(env));
    if (url.pathname.startsWith('/api/userbot/')) return handleUserbot(request, env, url);

    const match = SESSION_API.exec(url.pathname);
    if (match) return handleSessionApi(request, env, match[1], match[2] as 'state' | 'pick');

    return new Response('Not found', { status: 404 });
  },
} satisfies ExportedHandler<Env>;

async function handleWebhook(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
): Promise<Response> {
  if (request.method !== 'POST') return new Response('Method not allowed', { status: 405 });

  // The webhook URL is public, so without this header anyone could forge a join.
  if (request.headers.get('X-Telegram-Bot-Api-Secret-Token') !== env.WEBHOOK_SECRET) {
    return new Response('Forbidden', { status: 403 });
  }

  // Guarded: an unparseable body used to escape as an uncaught exception, which Cloudflare
  // answers with a 500 "Worker threw a JavaScript exception" (error 1101) — the exact
  // symptom of a hand-made test whose JSON got mangled by a shell. Telegram itself always
  // sends valid JSON, so this is a caller error, not a retryable one.
  let update: TgUpdate;
  try {
    update = (await request.json()) as TgUpdate;
  } catch (err) {
    console.error('webhook body was not JSON', err instanceof Error ? err.message : err);
    return new Response('Bad request: body is not valid JSON', { status: 400 });
  }
  if (!update || typeof update !== 'object') return new Response('Bad request', { status: 400 });

  // Answer before doing the work: Telegram treats anything but a 2xx as a failure and
  // re-sends, which would repeat the Bot API calls. `waitUntil` keeps us alive for them.
  ctx.waitUntil(dispatch(env, update));
  return new Response('ok');
}

/**
 * `/telegram` is a verification page, so a link without a live session must never reach the
 * game. `/` stays the public demo: it carries no session and therefore can never unmute anyone.
 */
async function serveGame(request: Request, env: Env, url: URL): Promise<Response> {
  const sessionId = url.searchParams.get('session') ?? '';
  if (!SESSION_ID.test(sessionId)) {
    return invalidSession('This link has no session id, or it is malformed.', 400);
  }

  const snapshot = await env.SESSION.get(env.SESSION.idFromName(sessionId)).snapshot();
  if (!snapshot) {
    return invalidSession('Unknown session. Send /start to the bot to get a fresh link.', 404);
  }
  // Closed sessions keep their outcome (that is how the page shows "you were removed"), but
  // they must not start a new game. A PM verification has its own wording: a stranger who
  // failed has no /start to send, because the userbot is not a Bot API bot at all.
  if (snapshot.status !== 'open') {
    if (snapshot.userbot) {
      return snapshot.status === 'success'
        ? outcomePage('Verify successful', 'You have passed verification.', 200)
        : outcomePage('Verify failed', 'You have failed verification.', 410);
    }
    return invalidSession(
      `This verification already ended (${snapshot.status}). Send /start to the bot if you need to be verified again.`,
      410,
    );
  }

  return env.ASSETS.fetch(new URL('/index.html', request.url).toString());
}

/**
 * Rendered here rather than shipped as an asset, because the reason differs per case and it has
 * to look like the game's own dark page — a member in Telegram's in-app browser should not be
 * left staring at a blank screen. The status codes are honest (400/404/410); browsers render
 * this body either way.
 */
function invalidSession(detail: string, status: number): Response {
  return outcomePage('Invalid Session', detail, status);
}

/** A dark status page; the PM verdict uses the exact wording the userbot promises. */
function outcomePage(heading: string, detail: string, status: number): Response {
  const html = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(heading)}</title>
<style>
  html, body { height: 100%; margin: 0; background: #1b1b1f; color: #f0f0f0;
    font-family: system-ui, sans-serif; display: flex; align-items: center; justify-content: center; }
  main { max-width: 32rem; padding: 1.5rem; text-align: center; }
  h1 { font-size: 1.25rem; margin: 0 0 0.75rem; }
  p { margin: 0; line-height: 1.5; color: #c9c9d1; }
</style>
</head>
<body>
<main>
  <h1>${escapeHtml(heading)}</h1>
  <p>${escapeHtml(detail)}</p>
</main>
</body>
</html>`;
  return new Response(html, {
    status,
    headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' },
  });
}

/**
 * The authenticated userbot API.
 *
 * The userbot is an MTProto user account, not a Bot API bot: it cannot receive a webhook
 * and the Worker cannot call it. So it drives the Worker (create a session, report the
 * message it sent) and polls the inbox for the cleanups it has to perform when somebody
 * passes. The bearer token is derived from the account's api_hash, which is the one
 * secret both halves already share.
 */
async function handleUserbot(request: Request, env: Env, url: URL): Promise<Response> {
  if (binding(env.USERBOT_API_HASH) !== 'set') {
    return json({ error: 'userbot-not-configured' }, 503);
  }
  if (!(await userbotAuthorized(request, env))) {
    return json({ error: 'unauthorized' }, 401);
  }

  if (url.pathname === '/api/userbot/session') {
    if (request.method !== 'POST') return methodNotAllowed();
    return createUserbotSession(request, env);
  }

  if (url.pathname === '/api/userbot/actions') {
    if (request.method !== 'GET') return methodNotAllowed();
    const inbox = env.USERBOT_INBOX.get(env.USERBOT_INBOX.idFromName(USERBOT_INBOX_NAME));
    return json({ actions: await inbox.list() });
  }

  if (url.pathname === '/api/userbot/heartbeat') {
    const inbox = env.USERBOT_INBOX.get(env.USERBOT_INBOX.idFromName(USERBOT_INBOX_NAME));
    if (request.method === 'GET') return json({ lastSeen: await inbox.lastSeen() });
    if (request.method !== 'POST') return methodNotAllowed();
    await inbox.heartbeat();
    return json({ ok: true });
  }

  const action = USERBOT_ACTION.exec(url.pathname);
  if (action) {
    if (request.method !== 'POST') return methodNotAllowed();
    const inbox = env.USERBOT_INBOX.get(env.USERBOT_INBOX.idFromName(USERBOT_INBOX_NAME));
    await inbox.ack(action[1]);
    return json({ ok: true });
  }

  const session = USERBOT_SESSION_API.exec(url.pathname);
  if (!session) return json({ error: 'not-found' }, 404);
  const stub = env.SESSION.get(env.SESSION.idFromName(session[1]));

  if (session[2] === 'abort') {
    if (request.method !== 'POST') return methodNotAllowed();
    await stub.abortUserbot();
    return json({ ok: true });
  }

  if (request.method !== 'POST') return methodNotAllowed();
  const body = (await request.json().catch(() => null)) as { messageId?: unknown } | null;
  const messageId = Number(body?.messageId);
  if (!Number.isSafeInteger(messageId) || messageId <= 0) {
    return json({ error: 'bad-request' }, 400);
  }
  const attached = await stub.attachUserbotMessage(messageId);
  if (!attached) return json({ error: 'no-open-userbot-session' }, 409);
  return json({ ok: true });
}

async function createUserbotSession(
  request: Request,
  env: Env,
): Promise<Response> {
  const body = (await request.json().catch(() => null)) as {
    userId?: unknown;
    userName?: unknown;
  } | null;
  const userId = Number(body?.userId);
  const userName =
    typeof body?.userName === 'string' ? body.userName.slice(0, 128) : String(userId);
  if (!Number.isSafeInteger(userId) || userId <= 0) {
    return json({ error: 'bad-request' }, 400);
  }

  const index = env.USER_INDEX.get(env.USER_INDEX.idFromName(userIndexName(userId)));
  // A user who already passed is not a stranger any more, even if the local history
  // looks empty (the verification message was deleted on success).
  if (await index.verified()) {
    return json({ sessionId: null, created: false, verified: true });
  }
  // Mirrors the join handler: `begin` can hand back a session that closes between the
  // two calls (its alarm firing is exactly that race), so one retry keeps a member from
  // being left without any session at all.
  for (let attempt = 0; attempt < 2; attempt++) {
    const candidate = crypto.randomUUID();
    const sessionId = await index.begin(
      candidate,
      Date.now() + (USERBOT_MUTE_SECONDS + 60) * 1000,
    );
    const result = await env.SESSION.get(env.SESSION.idFromName(sessionId)).ensureUserbot(
      userId,
      userName,
    );
    if (result.state.status !== 'open') {
      await index.finish(sessionId);
      continue;
    }

    // A chat attached to the same session can keep it alive longer than the PM window.
    const deadlines = Object.values(result.state.chats)
      .filter((chat) => chat.state === 'restricted')
      .map((chat) => chat.deadline);
    if (result.state.userbot) deadlines.push(result.state.userbot.deadline);
    await index.extend(sessionId, Math.max(...deadlines, Date.now()));

    return json({
      sessionId,
      created: result.created,
      status: result.state.status,
      // A session whose send never completed has no message id; the userbot may try
      // again. A non-null id means the link is already out, so it must not send another.
      messageId: result.state.userbot?.messageId ?? null,
    });
  }

  return json({ error: 'could not open a session' }, 503);
}

/**
 * The bearer the userbot has to present. Derived rather than sent raw: the api_hash is a
 * credential for the whole account, and a derived token is useless anywhere else.
 */
let tokenSecret = '';
let tokenValue: Promise<string> | null = null;

async function userbotToken(env: Env): Promise<string> {
  const secret = env.USERBOT_API_HASH?.trim() ?? '';
  if (secret && (tokenValue === null || tokenSecret !== secret)) {
    tokenSecret = secret;
    tokenValue = (async () => {
      const bytes = new TextEncoder().encode(`violetta-userbot:${secret}`);
      const digest = await crypto.subtle.digest('SHA-256', bytes);
      return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
    })();
  }
  return tokenValue ?? '';
}

async function userbotAuthorized(request: Request, env: Env): Promise<boolean> {
  const expected = await userbotToken(env);
  if (!expected) return false;
  const header = request.headers.get('authorization') ?? '';
  const presented = header.startsWith('Bearer ') ? header.slice('Bearer '.length) : '';
  return constantTimeEqual(presented, expected);
}

/** Both sides are hex of the same length; this only avoids an early-exit compare. */
function constantTimeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

const methodNotAllowed = () => new Response('Method not allowed', { status: 405 });

async function handleSessionApi(
  request: Request,
  env: Env,
  sessionId: string,
  action: 'state' | 'pick',
): Promise<Response> {
  const session = env.SESSION.get(env.SESSION.idFromName(sessionId));

  if (action === 'state') {
    if (request.method !== 'GET') return new Response('Method not allowed', { status: 405 });
    const snapshot = await session.snapshot();
    // A link that was already closed still reports its outcome (that is how the page shows
    // "you were removed"); only a session id that never existed is a 404.
    if (!snapshot) return json({ error: 'unknown-session' }, 404);
    return json(snapshot);
  }

  if (request.method !== 'POST') return new Response('Method not allowed', { status: 405 });

  const body = (await request.json().catch(() => null)) as {
    roundIndex?: unknown;
    slot?: unknown;
    elapsedMs?: unknown;
  } | null;
  const roundIndex = Number(body?.roundIndex);
  const slot = Number(body?.slot);
  const elapsedMs = Number(body?.elapsedMs ?? 0);

  if (!isRoundIndex(roundIndex) || !isSlot(slot) || !Number.isFinite(elapsedMs)) {
    return json({ error: 'bad-request' }, 400);
  }

  const result = await session.pick(roundIndex, slot, elapsedMs);
  if (!result.ok) {
    // Mirrors the page gate: a session that never existed is 404, one that already ended is
    // 410, and clicking out of turn is a conflict.
    const status = result.reason === 'unknown-session' ? 404 : result.reason === 'closed' ? 410 : 409;
    return json(result, status);
  }
  return json(result);
}

const isRoundIndex = (n: number) => Number.isInteger(n) && n >= 0 && n < 4;
/** 1-4 is a position; 0 is the page reporting a round the player ran out of time on. */
const isSlot = (n: number) => Number.isInteger(n) && n >= 0 && n <= 4;

/**
 * A deploy that forgot a secret or still carries the placeholder values looks completely
 * healthy otherwise — it only fails once a real member joins, as a mute notice whose
 * button goes nowhere. So report which pieces are actually there.
 *
 * `Boolean(env.BOT_TOKEN)` is not enough to answer that: an EMPTY secret is defined but
 * falsy, so a secret pushed with no value reads exactly like a missing one and the
 * difference is the entire diagnosis. Hence three states rather than a boolean.
 *
 * Presence only, never a value — and the two public values below are already visible in
 * every link the bot hands out.
 */
function binding(value: string | undefined): 'set' | 'empty' | 'missing' {
  if (value === undefined) return 'missing';
  // Whitespace alone counts as empty: a secret pasted with a trailing newline is spliced
  // into every API URL and cannot work, so it must not read as a healthy 'set' either.
  return value.trim().length === 0 ? 'empty' : 'set';
}

function health(env: Env): Record<string, unknown> {
  return {
    ok: true,
    config: {
      botToken: binding(env.BOT_TOKEN),
      webhookSecret: binding(env.WEBHOOK_SECRET),
      botUsername: env.BOT_USERNAME ?? null,
      verifyOrigin: env.VERIFY_ORIGIN ?? null,
      userbotApiHash: binding(env.USERBOT_API_HASH),
    },
  };
}
