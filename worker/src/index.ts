import type { Env } from './env';
import { dispatch } from './gating';
import { ChatRegistry } from './registry';
import { VerificationSession } from './session';
import type { TgUpdate } from './telegram';
import { UserIndex } from './user-index';

// Durable Object classes have to be exported from the Worker's entry point.
export { ChatRegistry, UserIndex, VerificationSession };

const SESSION_API =
  /^\/api\/session\/([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\/(state|pick)$/;

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
    if (url.pathname === '/telegram' && request.method === 'GET') return serveGame(request, env);
    if (url.pathname === '/api/health') return json({ ok: true });

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

  const update = (await request.json()) as TgUpdate;
  // Answer before doing the work: Telegram treats anything but a 2xx as a failure and
  // re-sends, which would repeat the Bot API calls. `waitUntil` keeps us alive for them.
  ctx.waitUntil(dispatch(env, update));
  return new Response('ok');
}

/** `?session=` is appended by the client; the bundle is the same file as `/`. */
async function serveGame(request: Request, env: Env): Promise<Response> {
  const indexUrl = new URL('/index.html', request.url).toString();
  return env.ASSETS.fetch(indexUrl);
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
  return json(await session.pick(roundIndex, slot, elapsedMs));
}

const isRoundIndex = (n: number) => Number.isInteger(n) && n >= 0 && n < 4;
const isSlot = (n: number) => Number.isInteger(n) && n >= 1 && n <= 4;
