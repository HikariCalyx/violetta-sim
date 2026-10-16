import type { Env } from './env';
import { dispatch } from './gating';
import { ChatRegistry } from './registry';
import { VerificationSession } from './session';
import type { TgUpdate } from './telegram';
import { escapeHtml } from './telegram';
import { UserIndex } from './user-index';

// Durable Object classes have to be exported from the Worker's entry point.
export { ChatRegistry, UserIndex, VerificationSession };

const UUID = '[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}';
const SESSION_API = new RegExp(`^/api/session/(${UUID})/(state|pick)$`);
const SESSION_ID = new RegExp(`^${UUID}$`);

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
  // they must not start a new game.
  if (snapshot.status !== 'open') {
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
  const html = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Invalid Session</title>
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
  <h1>Invalid Session</h1>
  <p>${escapeHtml(detail)}</p>
</main>
</body>
</html>`;
  return new Response(html, {
    status,
    headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' },
  });
}

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
    },
  };
}
