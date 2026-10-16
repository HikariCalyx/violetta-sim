import { DurableObject } from 'cloudflare:workers';
import { userIndexName, type Env } from './env';
import { ROUNDS_PER_ATTEMPT, answerSlot, makeRounds, type RoundSet } from './rounds';
import {
  FULL_PERMISSIONS,
  MAX_ATTEMPTS,
  MUTED_PERMISSIONS,
  MUTE_SECONDS,
  callTelegram,
  escapeHtml,
  mention,
  tryCallTelegram,
} from './telegram';

export type ChatEntryState =
  /** Muted and waiting; the alarm will kick this member. */
  | 'restricted'
  /** Finished the mini-game; the restriction was lifted. */
  | 'passed'
  /** Kicked by the deadline or by running out of attempts. */
  | 'removed'
  /** No longer our business: the member left, or we could not mute them at all. */
  | 'gone';

export interface ChatEntry {
  chatId: number;
  title: string;
  /** Unix ms, per chat: joining a second protected chat buys that chat its own 10 minutes. */
  deadline: number;
  /** Our own notice in that chat, deleted when the entry stops being `restricted`. */
  noticeId: number | null;
  state: ChatEntryState;
}

export type SessionStatus = 'open' | 'success' | 'failed' | 'expired';

export interface PickRecord {
  roundIndex: number;
  slot: number;
  elapsedMs: number;
  correct: boolean;
}

export interface SessionState {
  sessionId: string;
  userId: number;
  userName: string;
  status: SessionStatus;
  /** Failed games so far, shared by every chat in `chats`. */
  attempts: number;
  chats: Record<string, ChatEntry>;
  set: RoundSet;
  /** Round the page is on; resets to 0 for a retry. */
  roundIndex: number;
  picks: PickRecord[];
  createdAt: number;
  closedAt: number | null;
}

const KEY = 'state';

/**
 * How long a closed session stays readable. The link is dead either way — the page refuses
 * anything that is not `open` — but keeping the record for a day lets it say *why* it ended
 * ("already ended (expired)") instead of a bare "unknown session". After that the alarm
 * below wipes it, so completed verifications do not accumulate in storage forever.
 */
const CLOSED_RETENTION_MS = 24 * 60 * 60 * 1000;

/** The public view the game page reads. No secrets — the shuffle is visible on screen anyway. */
export interface SessionSnapshot {
  sessionId: string;
  status: SessionStatus;
  serverNow: number;
  /** Earliest kick, across the chats this verification still covers. */
  deadline: number | null;
  attempts: number;
  attemptsLeft: number;
  chats: { title: string; state: ChatEntryState }[];
  /** `rounds[i][character] = slot`, so the page can animate what the server scored. */
  rounds: number[][];
  special: number;
  roundIndex: number;
}

/** What `pick` reports back. A closed session is not an error the page should retry. */
export type PickResult =
  | { ok: false; reason: 'unknown-session' | 'closed' | 'out-of-order'; status?: SessionStatus }
  | {
      ok: true;
      correct: boolean;
      status: SessionStatus;
      attempts?: number;
      attemptsLeft: number;
      roundIndex?: number;
      /** The next attempt's shuffle, sent when this pick ended an attempt. */
      rounds?: number[][];
      special?: number;
    };

/**
 * One verification, covering every protected chat the member has joined.
 *
 * All of it lives in this single object on purpose: the countdown alarm, the join
 * handler and the game page all mutate the same state, and a Durable Object is
 * single-threaded per session, so a late "success" cannot race the deadline kick.
 */
export class VerificationSession extends DurableObject<Env> {
  /**
   * Everything that writes goes through this chain, one call at a time.
   *
   * A Durable Object serialises the *delivery* of requests but not the awaits inside them.
   * `ensure` reads its state and then spends two network round trips muting the member and
   * posting the notice; a second call arriving in that window — and one join does arrive
   * twice, as a `chat_member` update and as the "… joined the group" service message —
   * would read the state from before the first call saved anything and mute and greet the
   * member all over again. Chaining the writes is what makes the state machine in this
   * object as single-threaded as it looks.
   */
  private tail: Promise<unknown> = Promise.resolve();

  private serialize<T>(work: () => Promise<T>): Promise<T> {
    const next = this.tail.then(work, work);
    // A failure must not poison the chain for whoever comes next.
    this.tail = next.then(
      () => undefined,
      () => undefined,
    );
    return next;
  }

  /**
   * Adds a chat to this verification and mutes the member. Idempotent: also called when
   * the member joins a *second* protected chat, which must NOT reset `attempts`, re-issue
   * a link, or re-post a notice.
   */
  async ensure(
    userId: number,
    userName: string,
    chatId: number,
    title: string,
  ): Promise<SessionState> {
    return this.serialize(() => this.ensureLocked(userId, userName, chatId, title));
  }

  async pick(roundIndex: number, slot: number, elapsedMs: number): Promise<PickResult> {
    return this.serialize(() => this.pickLocked(roundIndex, slot, elapsedMs));
  }

  async memberLeft(chatId: number): Promise<void> {
    return this.serialize(() => this.memberLeftLocked(chatId));
  }

  /** The deadline, and the retention reaper that follows it. */
  async alarm(): Promise<void> {
    return this.serialize(() => this.alarmLocked());
  }

  async read(): Promise<SessionState | null> {
    return (await this.ctx.storage.get<SessionState>(KEY)) ?? null;
  }

  private async ensureLocked(
    userId: number,
    userName: string,
    chatId: number,
    title: string,
  ): Promise<SessionState> {
    let s = await this.read();
    if (!s) {
      s = {
        sessionId: this.ctx.id.name ?? crypto.randomUUID(),
        userId,
        userName,
        status: 'open',
        attempts: 0,
        chats: {},
        set: makeRounds(),
        roundIndex: 0,
        picks: [],
        createdAt: Date.now(),
        closedAt: null,
      };
    }
    // A closed session is the join handler's cue to start a new one.
    if (s.status !== 'open') return s;

    const key = String(chatId);
    const existing = s.chats[key];
    if (existing && (existing.state === 'restricted' || existing.state === 'passed')) return s;

    const entry: ChatEntry = {
      chatId,
      title,
      deadline: Date.now() + MUTE_SECONDS * 1000,
      noticeId: null,
      state: 'restricted',
    };
    s.chats[key] = entry;

    // Persisted and armed before the two network calls below: if either of them fails, or
    // the Worker dies in between, the member is muted with a session that will still kick
    // them — rather than muted with nothing to finish.
    await this.save(s);
    await this.arm(s);

    if (!(await this.mute(chatId, userId))) {
      // No rights, an admin target, a departing member... Never leave behind an entry the
      // alarm would later try to ban, and never post a notice we would not clean up.
      entry.state = 'gone';
      await this.save(s);
      await this.arm(s);
      return s;
    }

    try {
      entry.noticeId = await this.notice(userId, userName, chatId, title);
    } catch (err) {
      // The notice is only a courtesy: the entry stays `restricted`, so the deadline still
      // removes the member either way.
      console.error('notice failed', { chatId, userId }, err instanceof Error ? err.message : err);
      return s;
    }
    await this.save(s);
    return s;
  }

  /** The public view the game page reads. No secrets — the shuffle is visible on screen anyway. */
  async snapshot(): Promise<SessionSnapshot | null> {
    const s = await this.read();
    if (!s) return null;
    const restricted = Object.values(s.chats).filter((chat) => chat.state === 'restricted');
    return {
      sessionId: s.sessionId,
      status: s.status,
      serverNow: Date.now(),
      deadline: restricted.length ? Math.min(...restricted.map((chat) => chat.deadline)) : null,
      attempts: s.attempts,
      attemptsLeft: Math.max(0, MAX_ATTEMPTS - s.attempts),
      chats: Object.values(s.chats).map((chat) => ({ title: chat.title, state: chat.state })),
      rounds: s.set.rounds.map((round) => round.order),
      special: s.set.special,
      roundIndex: s.roundIndex,
    };
  }

  /**
   * One click, or a round the player let run out of time on.
   *
   * `slot` is 0 for "no answer": the page reports the round it timed out on, so the
   * attempt is scored either way instead of leaving one open that nothing would close.
   * The DO, not the page, decides whether that ends the attempt or the whole session.
   */
  private async pickLocked(
    roundIndex: number,
    slot: number,
    elapsedMs: number,
  ): Promise<PickResult> {
    const s = await this.read();
    if (!s) return { ok: false, reason: 'unknown-session' };
    if (s.status !== 'open') return { ok: false, reason: 'closed', status: s.status };
    if (roundIndex !== s.roundIndex) return { ok: false, reason: 'out-of-order' };

    const correct = slot === answerSlot(s.set, roundIndex);
    s.picks.push({ roundIndex, slot, elapsedMs, correct });
    const attemptsLeft = () => Math.max(0, MAX_ATTEMPTS - s.attempts);

    if (correct) {
      s.roundIndex += 1;
      if (s.roundIndex >= ROUNDS_PER_ATTEMPT) {
        await this.pass(s);
        await this.close(s, 'success');
        return { ok: true, correct: true, status: s.status, attemptsLeft: attemptsLeft() };
      }
      await this.save(s);
      return { ok: true, correct: true, status: s.status, roundIndex: s.roundIndex, attemptsLeft: attemptsLeft() };
    }

    s.attempts += 1;
    s.roundIndex = 0;
    const left = attemptsLeft();
    if (left <= 0) {
      for (const entry of Object.values(s.chats)) {
        if (entry.state === 'restricted') await this.evict(s, entry);
      }
      await this.close(s, 'failed');
      return { ok: true, correct: false, status: s.status, attempts: s.attempts, attemptsLeft: 0 };
    }

    // A new shuffle for the next attempt: replaying one the member has already been
    // scored on would hand them the answers for their remaining tries.
    s.set = makeRounds();
    await this.save(s);
    return {
      ok: true,
      correct: false,
      status: s.status,
      attempts: s.attempts,
      attemptsLeft: left,
      rounds: s.set.rounds.map((round) => round.order),
      special: s.set.special,
    };
  }

  /** The member left of their own accord: nothing to kick, just stop tracking them. */
  private async memberLeftLocked(chatId: number): Promise<void> {
    const s = await this.read();
    if (!s || s.status !== 'open') return;
    const entry = s.chats[String(chatId)];
    if (!entry || entry.state !== 'restricted') return;

    if (entry.noticeId) {
      await tryCallTelegram(this.env, 'deleteMessage', { chat_id: chatId, message_id: entry.noticeId });
    }
    entry.state = 'gone';
    entry.noticeId = null;
    await this.save(s);
    await this.arm(s);
    if (!this.anyRestricted(s)) await this.close(s, 'expired');
  }

  /**
   * A DO has exactly one alarm, so it is armed at the EARLIEST outstanding deadline and
   * re-armed on every fire — otherwise the second chat's deadline would never arrive.
   * Once the session is closed the same alarm is reused as the retention reaper.
   */
  private async alarmLocked(): Promise<void> {
    const s = await this.read();
    if (!s) return;
    if (s.status !== 'open') {
      await this.ctx.storage.deleteAll();
      return;
    }

    const now = Date.now();
    for (const entry of Object.values(s.chats)) {
      if (entry.state === 'restricted' && entry.deadline <= now) await this.evict(s, entry);
    }
    await this.save(s);
    await this.arm(s);
    if (!this.anyRestricted(s)) await this.close(s, 'expired');
  }

  private anyRestricted(s: SessionState): boolean {
    return Object.values(s.chats).some((chat) => chat.state === 'restricted');
  }

  private async save(s: SessionState): Promise<void> {
    await this.ctx.storage.put(KEY, s);
  }

  private async arm(s: SessionState): Promise<void> {
    const deadlines = Object.values(s.chats)
      .filter((chat) => chat.state === 'restricted')
      .map((chat) => chat.deadline);
    if (deadlines.length) {
      // A floor of "now + 1s" keeps a clock skew from firing an alarm in the past, which
      // a Durable Object would run immediately (and then again after each re-arm).
      await this.ctx.storage.setAlarm(Math.max(Math.min(...deadlines), Date.now() + 1000));
    } else {
      await this.ctx.storage.deleteAlarm();
    }
  }

  private async close(s: SessionState, status: SessionStatus): Promise<void> {
    s.status = status;
    s.closedAt = Date.now();
    await this.save(s);
    await this.ctx.storage.deleteAlarm();
    // Release the user: their next join must start a fresh verification, not resume this one.
    await this.env.USER_INDEX.get(
      this.env.USER_INDEX.idFromName(userIndexName(s.userId)),
    ).finish(s.sessionId);
    // Arm the reaper. The id can never be played again in the meantime: the page only serves
    // a game for `open` sessions.
    await this.ctx.storage.setAlarm(Date.now() + CLOSED_RETENTION_MS);
  }

  /** Success: lift the restriction everywhere it is still in force. */
  private async pass(s: SessionState): Promise<void> {
    for (const entry of Object.values(s.chats)) {
      // Chats whose own deadline already fired are skipped: the member is not there any
      // more, and an unban there would only hide the bug.
      if (entry.state !== 'restricted') continue;
      await tryCallTelegram(this.env, 'restrictChatMember', {
        chat_id: entry.chatId,
        user_id: s.userId,
        permissions: FULL_PERMISSIONS,
      });
      await this.dropNotice(entry);
      entry.state = 'passed';
    }
    await tryCallTelegram(this.env, 'sendMessage', {
      chat_id: s.userId,
      text: 'Verified — welcome! You can talk in the group now.',
    });
  }

  /**
   * Kick a member but let them come back and try again: a plain ban blocks every invite
   * link, so it is always ban followed by an immediate unban.
   */
  private async evict(s: SessionState, entry: ChatEntry): Promise<void> {
    await tryCallTelegram(this.env, 'banChatMember', {
      chat_id: entry.chatId,
      user_id: s.userId,
      revoke_messages: false,
    });
    await tryCallTelegram(this.env, 'unbanChatMember', {
      chat_id: entry.chatId,
      user_id: s.userId,
      only_if_banned: true,
    });
    await this.dropNotice(entry);
    entry.state = 'removed';
    await tryCallTelegram(this.env, 'sendMessage', {
      chat_id: s.userId,
      text: `You were removed from “${entry.title}” because verification was not completed. You can rejoin and try again.`,
    });
  }

  private async dropNotice(entry: ChatEntry): Promise<void> {
    if (entry.noticeId) {
      await tryCallTelegram(this.env, 'deleteMessage', {
        chat_id: entry.chatId,
        message_id: entry.noticeId,
      });
      entry.noticeId = null;
    }
  }

  private async mute(chatId: number, userId: number): Promise<boolean> {
    try {
      await callTelegram(this.env, 'restrictChatMember', {
        chat_id: chatId,
        user_id: userId,
        permissions: MUTED_PERMISSIONS,
        // until_date is left at 0 (forever) on purpose: it is lifted explicitly on success,
        // and Telegram reads any value under 30 seconds as "forever" anyway. The 10-minute
        // window is the alarm's job, not the restriction's.
      });
      return true;
    } catch (err) {
      console.error('mute failed', { chatId, userId }, err instanceof Error ? err.message : err);
      return false;
    }
  }

  private async notice(
    userId: number,
    userName: string,
    chatId: number,
    title: string,
  ): Promise<number | null> {
    const text =
      `Hi ${mention(userId, userName)}, welcome to ${escapeHtml(title)}!\n\n` +
      'To chat in this group, please play the minigame below.';
    const result = await callTelegram<{ message_id: number }>(this.env, 'sendMessage', {
      chat_id: chatId,
      text,
      parse_mode: 'HTML',
      link_preview_options: { is_disabled: true },
      // A URL button is the only way to open a PM: Telegram never lets a bot message
      // someone who has not started it. With no payload, this shows the START button to
      // members who never talked to the bot, and just opens the chat for those who have
      // — they have to send /start themselves in that case.
      reply_markup: {
        inline_keyboard: [[{ text: 'Start', url: `https://t.me/${this.env.BOT_USERNAME}` }]],
      },
    });
    return result.message_id;
  }

  /**
   * Whether this session is still holding the member muted in `chatId`.
   *
   * Used to tidy up after a join: anything a member manages to send before the mute
   * lands is deleted, and this is what decides whether a given message falls in that
   * window.
   */
  async mutedIn(chatId: number): Promise<boolean> {
    const s = await this.read();
    if (!s || s.status !== 'open') return false;
    return s.chats[String(chatId)]?.state === 'restricted';
  }
}
