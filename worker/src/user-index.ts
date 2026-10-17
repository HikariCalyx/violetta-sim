import { DurableObject } from 'cloudflare:workers';
import type { Env } from './env';
import type { CapturedMessage } from './telegram';

export interface ActiveSession {
  sessionId: string;
  /**
   * The LATEST deadline among the session's still-muted chats, so "the index says it is
   * expired" implies every chat's deadline has passed and the session is really dead.
   */
  expiresAt: number;
}

/**
 * How long a join marker is trusted. The mute normally lands within a second; the
 * window only has to outlive the gap before the session is written, not the whole
 * verification.
 */
const JOIN_MARKER_WINDOW_MS = 60 * 1000;

const JOIN_PREFIX = 'join:';
const CAPTURED_PREFIX = 'captured:';
const JOIN_HISTORY_PREFIX = 'joins:';
const VERIFIED = 'userbot-verified';

/** More joins than this, inside the window, is a burst rather than a new member. */
const JOIN_LIMIT = 3;
const JOIN_WINDOW_MS = 35 * 60 * 1000;
/**
 * One join arrives twice — as a `chat_member` update and as the "… joined the group"
 * service message — so a second event this soon after the last is the same join.
 */
const JOIN_DEDUPE_MS = 10 * 1000;
/** Only enough history to know the limit has been passed is kept. */
const JOIN_HISTORY_KEEP = JOIN_LIMIT + 5;

/**
 * One object per user, holding *which* verification session is currently open for them.
 *
 * It exists so a plain UUID in the page URL can find the user's state without a
 * session-id → user-id lookup table, and so the join handler can decide atomically
 * between "this user is already being verified, add the chat to that session" and
 * "start a new one".
 *
 * It also holds the join marker and the messages captured in the moment before a mute
 * lands. The webhook runs updates concurrently, so a message can arrive while the join
 * handler is still finding out whether the chat is gated — before the session is
 * written. The marker is what lets that message be recognised, and the capture is what
 * carries it into the session that is about to be created.
 */
export class UserIndex extends DurableObject<Env> {
  /**
   * `recordJoin` is a read-modify-write of one list, and the two updates for one join
   * can arrive at the same moment, so those calls are chained rather than left to race.
   */
  private joinTail: Promise<unknown> = Promise.resolve();

  private serializeJoins<T>(work: () => Promise<T>): Promise<T> {
    const next = this.joinTail.then(work, work);
    this.joinTail = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  }

  async active(): Promise<ActiveSession | null> {
    const entry = await this.ctx.storage.get<ActiveSession>('active');
    if (!entry) return null;
    if (entry.expiresAt <= Date.now()) {
      await this.ctx.storage.delete('active');
      return null;
    }
    return entry;
  }

  /**
   * Returns the session id the caller must use: the open one if there is one, otherwise
   * `candidate` (which the caller then creates). Serialised by the DO, so two joins that
   * race cannot both win.
   */
  async begin(candidate: string, expiresAt: number): Promise<string> {
    const open = await this.active();
    if (open) return open.sessionId;
    await this.ctx.storage.put('active', { sessionId: candidate, expiresAt });
    return candidate;
  }

  /** Push the expiry out as more protected chats are added to the same session. */
  async extend(sessionId: string, expiresAt: number): Promise<void> {
    const entry = await this.ctx.storage.get<ActiveSession>('active');
    if (entry?.sessionId === sessionId) await this.ctx.storage.put('active', { sessionId, expiresAt });
  }

  /** Only clear the pointer if it still belongs to us: a newer session must survive. */
  async finish(sessionId: string): Promise<void> {
    const entry = await this.ctx.storage.get<ActiveSession>('active');
    if (entry?.sessionId === sessionId) await this.ctx.storage.delete('active');
  }

  /** Records that the member has just joined `chatId`, before the mute is even sent. */
  async noteJoin(chatId: number): Promise<void> {
    await this.ctx.storage.put(`${JOIN_PREFIX}${chatId}`, Date.now());
  }

  /** The mute has landed (or the join was not ours to gate): stop trusting the marker. */
  async clearJoin(chatId: number): Promise<void> {
    await this.ctx.storage.delete(`${JOIN_PREFIX}${chatId}`);
  }

  /** Whether the member joined `chatId` moments ago, so a message may predate the mute. */
  async joinedRecently(chatId: number): Promise<boolean> {
    const at = await this.ctx.storage.get<number>(`${JOIN_PREFIX}${chatId}`);
    return at !== undefined && Date.now() - at <= JOIN_MARKER_WINDOW_MS;
  }

  /**
   * Records one join of `chatId` and reports whether the member is joining over and
   * over: more than three times inside 35 minutes.
   *
   * The history outlives a verification on purpose — that is what makes a rejoining
   * spam bot recognisable on the join *after* the first failed attempt.
   */
  async recordJoin(chatId: number): Promise<boolean> {
    return this.serializeJoins(async () => {
      const key = `${JOIN_HISTORY_PREFIX}${chatId}`;
      const now = Date.now();
      const stored = (await this.ctx.storage.get<number[]>(key)) ?? [];
      const recent = stored.filter((at) => now - at <= JOIN_WINDOW_MS);

      // The duplicate update for the same join must not count twice.
      if (recent.length && now - recent[recent.length - 1] < JOIN_DEDUPE_MS) {
        await this.ctx.storage.put(key, recent);
        return recent.length > JOIN_LIMIT;
      }

      recent.push(now);
      const kept = recent.slice(-JOIN_HISTORY_KEEP);
      await this.ctx.storage.put(key, kept);
      return kept.length > JOIN_LIMIT;
    });
  }

  /**
   * Records that this user passed the PM userbot's verification.
   *
   * The userbot's only local state is a StringSession, and the message that proved the
   * check is deleted on success. Without this marker, a later restart that sweeps recent
   * dialogs — or a user who clears their own history — would look like a first contact
   * and be gated again.
   */
  async markVerified(): Promise<void> {
    await this.ctx.storage.put(VERIFIED, true);
  }

  /** Whether this user has already passed a PM verification. */
  async verified(): Promise<boolean> {
    return (await this.ctx.storage.get<boolean>(VERIFIED)) === true;
  }

  /**
   * The chat turned out not to be gated, so neither the marker nor anything captured
   * for it is our business after all.
   */
  async discardJoin(chatId: number): Promise<void> {
    await this.clearJoin(chatId);
    await this.discardCaptured(chatId);
  }

  /**
   * Keeps a message captured before the mute landed (or a guest bot's answer after it),
   * until a session exists to adopt it.
   *
   * One key per message rather than one object holding them all: a read-modify-write
   * list could lose a capture when a message and the session's adoption run together.
   */
  async capture(chatId: number, message: CapturedMessage): Promise<void> {
    await this.ctx.storage.put(`${CAPTURED_PREFIX}${chatId}:${message.messageId}:${message.kind}`, message);
  }

  /** Hands every captured message over to the session that is adopting them. */
  async takeCaptured(): Promise<CapturedMessage[]> {
    const stored = await this.ctx.storage.list<CapturedMessage>({ prefix: CAPTURED_PREFIX });
    if (stored.size) await this.ctx.storage.delete([...stored.keys()]);
    return [...stored.values()];
  }

  /** Drops captured messages, all of them or only one chat's. */
  async discardCaptured(chatId?: number): Promise<void> {
    const prefix = chatId === undefined ? CAPTURED_PREFIX : `${CAPTURED_PREFIX}${chatId}:`;
    const stored = await this.ctx.storage.list({ prefix });
    if (stored.size) await this.ctx.storage.delete([...stored.keys()]);
  }
}
