import { REGISTRY_NAME, newSessionId, userIndexName, type Env, type RegisteredChat } from './env';
import { type SessionState, type VerificationSession } from './session';
import type {
  CapturedMessage,
  TgChat,
  TgChatMember,
  TgChatMemberUpdated,
  TgMessage,
  TgUpdate,
  TgUser,
} from './telegram';
import { CHAOS_MUTE_SECONDS, callTelegram, displayName, mmss, tryCallTelegram } from './telegram';
import type { ChatRegistry } from './registry';
import type { UserIndex } from './user-index';

const registryStub = (env: Env): DurableObjectStub<ChatRegistry> =>
  env.REGISTRY.get(env.REGISTRY.idFromName(REGISTRY_NAME));

const indexStub = (env: Env, userId: number): DurableObjectStub<UserIndex> =>
  env.USER_INDEX.get(env.USER_INDEX.idFromName(userIndexName(userId)));

const sessionStub = (env: Env, sessionId: string): DurableObjectStub<VerificationSession> =>
  env.SESSION.get(env.SESSION.idFromName(sessionId));

/**
 * Only a `left|kicked → member|restricted` transition is a join. Our own mute turns
 * `member → restricted`, so without this guard every gated member would immediately
 * look like a brand-new one.
 */
export function joinKind(update: TgChatMemberUpdated): 'join' | 'leave' | null {
  const before = update.old_chat_member.status;
  const after = update.new_chat_member.status;
  const wasOut = before === 'left' || before === 'kicked';
  const isIn = after === 'member' || after === 'restricted';
  if (wasOut && isIn) return 'join';
  if (!wasOut && !isIn) return 'leave';
  return null;
}

/** The index needs the LATEST deadline so that "expired" means every chat is done. */
function furthestDeadline(state: SessionState): number {
  const deadlines = Object.values(state.chats)
    .filter((chat) => chat.state === 'restricted')
    .map((chat) => chat.deadline);
  if (state.userbot) deadlines.push(state.userbot.deadline);
  return deadlines.length ? Math.max(...deadlines) : Date.now();
}

/**
 * Whether this chat should be gated, and the registry entry for it.
 *
 * The bot gates every chat where it holds the rights it needs, so nobody has to enrol one:
 * being made an admin is the whole opt-in. The first join in a chat the bot has not seen
 * before is what confirms the rights, because a promotion that happened before this Worker
 * existed sends no update of its own.
 */
async function gatedChat(env: Env, chat: TgChat): Promise<RegisteredChat | null> {
  const registry = registryStub(env);
  const known = await registry.get(chat.id);
  if (known) return known;

  if (!(await botRights(env, chat.id))) return null;
  console.info('gating this chat: the bot has the rights it needs', { chatId: chat.id });
  return registry.remember(chat.id, chat.title ?? 'the group');
}

/**
 * Confirms the bot is an administrator in `chatId` with the two rights it needs.
 *
 * There is no "Ban users" permission in the Bot API: muting and kicking are both
 * `can_restrict_members`, which is what the clients label that way. Telegram omits
 * permission fields that are false, so they are compared to `true`.
 */
async function botRights(env: Env, chatId: number): Promise<boolean> {
  try {
    const registry = registryStub(env);
    let botId = await registry.botId();
    if (botId === null) {
      botId = (await callTelegram<TgUser>(env, 'getMe', {})).id;
      await registry.setBotId(botId);
    }

    const self = await callTelegram<TgChatMember>(env, 'getChatMember', {
      chat_id: chatId,
      user_id: botId,
    });
    return (
      self.status === 'administrator' &&
      self.can_restrict_members === true &&
      self.can_delete_messages === true
    );
  } catch (err) {
    console.error(
      'could not read the bot rights here',
      { chatId },
      err instanceof Error ? err.message : err,
    );
    return false;
  }
}

/**
 * Mutes a member who has just joined, and reports whether this chat is one the bot
 * gates. The marker for the message window is written before the gating check, so the
 * caller can use the same moment to decide whether to clear the arrival notice.
 */
export async function onMemberJoined(env: Env, chat: TgChat, user: TgUser): Promise<boolean> {
  if (user.is_bot) return false;

  const index = indexStub(env, user.id);
  // Marked before the gating check, and therefore before any network round trip: update
  // processing is concurrent, so a message the member sends in this window may be
  // handled before the session they belong to has even been written.
  await index.noteJoin(chat.id);

  const registered = await gatedChat(env, chat);
  if (!registered) {
    // Not a chat this bot gates, so nothing may be captured there either.
    await index.discardJoin(chat.id);
    return false;
  }

  const userName = displayName(user);
  const title = registered.title || chat.title || 'the group';
  // More than three joins in 35 minutes is a burst, not a new member on their way in.
  const burst = await index.recordJoin(chat.id);

  try {
    // Two attempts: the session handed back by `begin` can close between `begin` and
    // `ensure` (its alarm firing is exactly that race). Starting a new one keeps a member
    // from being left with no session at all — i.e. muted forever with nothing to finish.
    for (let attempt = 0; attempt < 2; attempt++) {
      const candidate = newSessionId();
      // The index only needs an upper bound until `ensure` reports the real deadline, so
      // the longer chaos window is the safe one here.
      const sessionId = await index.begin(candidate, Date.now() + (CHAOS_MUTE_SECONDS + 60) * 1000);
      const state = await sessionStub(env, sessionId).ensure(
        user.id,
        userName,
        chat.id,
        title,
        burst,
      );
      if (state.status === 'open') {
        await index.extend(sessionId, furthestDeadline(state));
        return true;
      }
      await index.finish(sessionId);
    }
    console.error('could not open a verification session', { chatId: chat.id, userId: user.id });
    return false;
  } finally {
    // Past `ensure` the session itself recognises the member, whether or not the mute
    // call succeeded, so the marker has done its job.
    await index.clearJoin(chat.id);
  }
}

async function onMemberLeft(env: Env, chatId: number, userId: number): Promise<void> {
  const active = await indexStub(env, userId).active();
  if (!active) return;
  await sessionStub(env, active.sessionId).memberLeft(chatId);
}

/**
 * The context recorded for a message the bot had to remove: small enough to keep, and
 * enough to recognise what it was. The text is truncated — this is a note, not an
 * archive of the chat.
 */
function capturedMessage(msg: TgMessage, kind: CapturedMessage['kind']): CapturedMessage {
  const text = msg.text ?? msg.caption ?? '';
  return {
    kind,
    chatId: msg.chat.id,
    messageId: msg.message_id,
    date: msg.date,
    text: text ? text.slice(0, 1000) : undefined,
    bot: msg.via_bot?.username ?? (msg.guest_bot_caller_user ? msg.from?.username : undefined),
  };
}

/**
 * Records a message the member should not have been able to leave in the chat, then
 * deletes it.
 *
 * Recorded first: a failed delete (no rights, another bot's message) must not lose the
 * fact that the member posted before the mute landed. Until the session exists, the
 * capture waits in the user's index and `ensure` adopts it.
 */
async function captureAndDelete(
  env: Env,
  userId: number,
  msg: TgMessage,
  kind: CapturedMessage['kind'],
): Promise<void> {
  const message = capturedMessage(msg, kind);
  const active = await indexStub(env, userId).active();
  const stored = active ? await sessionStub(env, active.sessionId).noteSuspicious(message) : false;
  if (!stored) await indexStub(env, userId).capture(msg.chat.id, message);

  console.info('captured a message from a member under verification', {
    chatId: msg.chat.id,
    userId,
    messageId: msg.message_id,
    kind,
  });

  await tryCallTelegram(env, 'deleteMessage', {
    chat_id: msg.chat.id,
    message_id: msg.message_id,
  });
}

/**
 * Handles one message in a group: the arrival notice, and anything a still-muted member
 * managed to send.
 *
 * Both need the bot to be able to SEE the message, and to delete it. Telegram withholds
 * ordinary group messages from a bot that is in privacy mode, so that mode is turned off
 * (see the README), and deleting needs the bot's `can_delete_messages` right. A guest
 * bot's answer is a message from another bot, which needs Bot-to-Bot Communication Mode.
 */
async function onGroupMessage(env: Env, msg: TgMessage): Promise<void> {
  if (msg.new_chat_members?.length) {
    // Fallback for chats that do not deliver `chat_member` updates. `ensure` is
    // idempotent, so both paths firing for one join cannot double-mute or double-post.
    // The join marker is written inside each call, before it checks the gating rights.
    let gated = false;
    for (const member of msg.new_chat_members) {
      gated = (await onMemberJoined(env, msg.chat, member)) || gated;
    }
    if (!gated) return;
    // "X joined the group": the trail of who arrived (and later, who was removed) is not
    // something the group needs to keep, and it is the first thing to clear for the
    // member who has just arrived. This is not a message the member sent, so nothing is
    // captured for it.
    await tryCallTelegram(env, 'deleteMessage', {
      chat_id: msg.chat.id,
      message_id: msg.message_id,
    });
    return;
  }

  // A guest bot answers in the chat on behalf of the member who summoned it: `from` is
  // the bot, but `guest_bot_caller_user` is the member. A muted member can still get a
  // message in this way, so a member under verification has these deleted too, and the
  // capture is what puts their challenge into chaos mode.
  const guestCaller = msg.guest_bot_caller_user;
  if (guestCaller && !guestCaller.is_bot) {
    const index = indexStub(env, guestCaller.id);
    const active = await index.active();
    const mutedHere = active ? await sessionStub(env, active.sessionId).mutedIn(msg.chat.id) : false;
    // ...or the mute has not been written yet, in which case the join marker stands in.
    const beforeMute = !mutedHere && (await index.joinedRecently(msg.chat.id));
    if (mutedHere || beforeMute) {
      await captureAndDelete(env, guestCaller.id, msg, 'guest-bot');
    }
    return;
  }

  const from = msg.from;
  if (!from || from.is_bot) return;

  // Only a member with a live verification can be muted, and that is almost nobody — so
  // this single lookup is the whole cost of the bot for an ordinary message. Anything
  // they manage to send before the restriction lands is captured and deleted; the join
  // marker covers the moment before the session itself exists.
  const muted = await isMuted(env, msg.chat.id, from.id);
  const beforeMute = muted || (await indexStub(env, from.id).joinedRecently(msg.chat.id));
  if (!beforeMute) return;

  await captureAndDelete(env, from.id, msg, 'before-mute');
}

/** Whether `userId` is currently muted in `chatId` by a live verification. */
async function isMuted(env: Env, chatId: number, userId: number): Promise<boolean> {
  const active = await indexStub(env, userId).active();
  if (!active) return false;
  return sessionStub(env, active.sessionId).mutedIn(chatId);
}

async function onChatMember(env: Env, update: TgChatMemberUpdated): Promise<void> {
  const kind = joinKind(update);
  if (!kind) return;
  const user = update.new_chat_member.user ?? update.from;
  if (user.is_bot) return;
  if (kind === 'join') await onMemberJoined(env, update.chat, user);
  else await onMemberLeft(env, update.chat.id, user.id);
}

async function onBotMembership(env: Env, update: TgChatMemberUpdated): Promise<void> {
  // `my_chat_member` is only ever about this bot, so its own permissions are already in the
  // update and no getMe/getChatMember round trip is needed here.
  const chatId = update.chat.id;
  const self = update.new_chat_member;
  const registry = registryStub(env);
  const known = await registry.get(chatId);

  if (self.status === 'left' || self.status === 'kicked') {
    if (known) {
      await registry.forget(chatId);
      console.warn('the bot was removed from a chat it was gating', { chatId });
    }
    return;
  }

  const enough =
    self.status === 'administrator' &&
    self.can_restrict_members === true &&
    self.can_delete_messages === true;

  if (enough) {
    // Promoted: gate from now on, without anybody having to ask.
    await registry.remember(chatId, update.chat.title ?? 'the group');
    console.info('gating this chat now that the bot is an admin', { chatId });
    return;
  }

  if (known) {
    await registry.forget(chatId);
    console.warn(
      'the bot no longer has the rights it needs here: promote it back with "Ban users" and "Delete messages"',
      { chatId },
    );
  }
}

/**
 * `/start` in PM — and, because the notice carries no payload, the only way in.
 * The user is looked up by id, so a forwarded deep link buys nothing.
 */
async function onPrivateStart(env: Env, user: TgUser, chatId: number): Promise<void> {
  const reply = (text: string, button?: { text: string; url: string }) =>
    callTelegram(env, 'sendMessage', {
      chat_id: chatId,
      text,
      parse_mode: 'HTML',
      link_preview_options: { is_disabled: true },
      ...(button ? { reply_markup: { inline_keyboard: [[button]] } } : {}),
    });

  const index = indexStub(env, user.id);
  const active = await index.active();
  if (!active) {
    await reply("You don't have any pending verifications.");
    return;
  }

  const state = await sessionStub(env, active.sessionId).read();
  const restricted = state && state.status === 'open'
    ? Object.values(state.chats).filter((chat) => chat.state === 'restricted')
    : [];
  if (!state || !restricted.length) {
    await index.finish(active.sessionId);
    await reply("You don't have any pending verifications.");
    return;
  }

  // The earliest deadline, because that is the one that actually removes them.
  const deadline = Math.min(...restricted.map((chat) => chat.deadline));
  // The play page as a button rather than a bare link: a URL button is what Telegram
  // shows as the thing to press, and the text stays a status line that fits next to it.
  await reply(`Please finish verification here. Time left: ${mmss(deadline - Date.now())}`, {
    text: 'Verify',
    url: `${env.VERIFY_ORIGIN}/telegram?session=${state.sessionId}`,
  });
}

/** Entry point for a webhook update. Never throws: a retried update would duplicate work. */
export async function dispatch(env: Env, update: TgUpdate): Promise<void> {
  try {
    if (update.chat_member) {
      await onChatMember(env, update.chat_member);
      return;
    }
    if (update.my_chat_member) {
      await onBotMembership(env, update.my_chat_member);
      return;
    }

    const msg = update.message;
    if (!msg) return;

    if (msg.chat.type === 'private') {
      if (msg.from && msg.text?.startsWith('/start')) await onPrivateStart(env, msg.from, msg.chat.id);
      return;
    }

    // The group path sorts out the arrival notice, messages bots post (a guest bot's
    // answer is one) and ordinary members alike, so it sees every group update.
    await onGroupMessage(env, msg);
  } catch (err) {
    console.error('update failed', update.update_id, err instanceof Error ? err.message : err);
  }
}
