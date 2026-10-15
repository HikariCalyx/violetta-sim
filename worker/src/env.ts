import type { ChatRegistry } from './registry';
import type { VerificationSession } from './session';
import type { UserIndex } from './user-index';

/** Bindings declared in `wrangler.jsonc`. */
export interface Env {
  /** Secret. The bot token from @BotFather — never log it, never send it to a browser. */
  BOT_TOKEN: string;
  /** Secret. The `secret_token` handed to setWebhook; the webhook refuses requests without it. */
  WEBHOOK_SECRET: string;
  /** Var. The bot's username, without the leading `@`. */
  BOT_USERNAME: string;
  /** Var. Where this Worker is reachable, e.g. `https://violetta.example.com`. */
  VERIFY_ORIGIN: string;

  /** The trunk-built game bundle (`../dist`). */
  ASSETS: Fetcher;
  REGISTRY: DurableObjectNamespace<ChatRegistry>;
  USER_INDEX: DurableObjectNamespace<UserIndex>;
  SESSION: DurableObjectNamespace<VerificationSession>;
}

/** A chat that gates its new members. */
export interface RegisteredChat {
  chatId: number;
  title: string;
  registeredAt: number;
}

/**
 * One Durable Object holds the whole registry: joins are rare, so serialising them
 * costs nothing and avoids a D1/KV round trip on the hot path (KV is eventually
 * consistent, which would let a chat be registered yet still miss the next join).
 */
export const REGISTRY_NAME = 'registry';

/** Per-user index. Keyed on the user alone — the gate is per user, not per chat. */
export const userIndexName = (userId: number) => `u:${userId}`;

/** A session id is a UUIDv4: unguessable, and the page's only credential. */
export const newSessionId = () => crypto.randomUUID();
