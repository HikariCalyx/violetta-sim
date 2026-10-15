import { DurableObject } from 'cloudflare:workers';
import type { Env } from './env';

export interface ActiveSession {
  sessionId: string;
  /**
   * The LATEST deadline among the session's still-muted chats, so "the index says it is
   * expired" implies every chat's deadline has passed and the session is really dead.
   */
  expiresAt: number;
}

/**
 * One object per user, holding *which* verification session is currently open for them.
 *
 * It exists so a plain UUID in the page URL can find the user's state without a
 * session-id → user-id lookup table, and so the join handler can decide atomically
 * between "this user is already being verified, add the chat to that session" and
 * "start a new one".
 */
export class UserIndex extends DurableObject<Env> {
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
}
