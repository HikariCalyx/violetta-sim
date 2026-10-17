import { DurableObject } from 'cloudflare:workers';
import type { Env } from './env';

/**
 * A cleanup the PM userbot has to perform the next time it is awake.
 *
 * The userbot is an MTProto user account, so the Bot API cannot unblock, unarchive or
 * delete a message for it. The session object is the one that knows a verification
 * passed, so it leaves this note here and the userbot drains the inbox on its own
 * schedule. A note survives the userbot being offline, which is exactly why it is
 * stored instead of called back to.
 */
export interface UserbotAction {
  id: string;
  /** What happened. Only a pass needs the blocked chat to be reopened. */
  kind: 'verified';
  sessionId: string;
  userId: number;
  /** The userbot's own message carrying the link, or null if it was never recorded. */
  messageId: number | null;
  createdAt: number;
}

const PREFIX = 'a:';
const HEARTBEAT = 'heartbeat';

/**
 * The userbot's outbox.
 *
 * Enqueue and delete are chained: the pass runs from a Durable Object alarm or a pick
 * request, and the userbot polls from a completely different request path, so a list
 * that ran while an enqueue was halfway through a storage write could miss the new
 * note. Reads wait behind a write here, but the userbot is a polling client and the
 * queue is tiny, so the wait is irrelevant.
 */
export class UserbotInbox extends DurableObject<Env> {
  private tail: Promise<unknown> = Promise.resolve();

  private serialize<T>(work: () => Promise<T>): Promise<T> {
    const next = this.tail.then(work, work);
    this.tail = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  }

  /**
   * Queues one cleanup. A session can only pass once, but a retried save must not
   * leave two notes for the same session — the second action would just repeat the
   * same idempotent calls, but keeping one makes the inbox easier to reason about.
   */
  async enqueue(action: Omit<UserbotAction, 'id' | 'createdAt'>): Promise<UserbotAction> {
    return this.serialize(async () => {
      const stored = await this.ctx.storage.list<UserbotAction>({ prefix: PREFIX });
      const existing = [...stored.values()].find((entry) => entry.sessionId === action.sessionId);
      if (existing) return existing;

      const entry: UserbotAction = { ...action, id: crypto.randomUUID(), createdAt: Date.now() };
      await this.ctx.storage.put(`${PREFIX}${entry.id}`, entry);
      return entry;
    });
  }

  /** Oldest first, so a backlog is worked through in the order it happened. */
  async list(): Promise<UserbotAction[]> {
    return this.serialize(async () => {
      const stored = await this.ctx.storage.list<UserbotAction>({ prefix: PREFIX });
      return [...stored.values()].sort((a, b) => a.createdAt - b.createdAt);
    });
  }

  /** The userbot confirms it has finished (or permanently given up on) one note. */
  async ack(id: string): Promise<void> {
    return this.serialize(async () => {
      await this.ctx.storage.delete(`${PREFIX}${id}`);
    });
  }

  /**
   * The last moment a userbot run was known to be alive.
   *
   * A GitHub Actions hand-off leaves a short gap in which nobody is connected, and the
   * MTProto session string carries no update cursor. The next run reads this marker and
   * sweeps only the messages that arrived after it, instead of re-examining old chats
   * (which would re-gate somebody who already passed).
   */
  async lastSeen(): Promise<number | null> {
    return this.serialize(async () => (await this.ctx.storage.get<number>(HEARTBEAT)) ?? null);
  }

  async heartbeat(): Promise<void> {
    return this.serialize(async () => {
      await this.ctx.storage.put(HEARTBEAT, Date.now());
    });
  }
}
