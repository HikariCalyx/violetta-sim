import { REGISTRY_NAME, newSessionId, userIndexName, type Env, type RegisteredChat } from './env';
import type { SessionState, VerificationSession } from './session';
import type {
  TgChat,
  TgChatMember,
  TgChatMemberUpdated,
  TgMessage,
  TgUpdate,
  TgUser,
} from './telegram';
import { MAX_ATTEMPTS, MUTE_SECONDS, callTelegram, displayName, mmss, tryCallTelegram } from './telegram';
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

export async function onMemberJoined(env: Env, chat: TgChat, user: TgUser): Promise<void> {
  if (user.is_bot) return;

  const registered = await gatedChat(env, chat);
  if (!registered) return;

  const index = indexStub(env, user.id);
  const userName = displayName(user);
  const title = registered.title || chat.title || 'the group';

  // Two attempts: the session handed back by `begin` can close between `begin` and
  // `ensure` (its alarm firing is exactly that race). Starting a new one keeps a member
  // from being left with no session at all — i.e. muted forever with nothing to finish.
  for (let attempt = 0; attempt < 2; attempt++) {
    const candidate = newSessionId();
    const sessionId = await index.begin(candidate, Date.now() + (MUTE_SECONDS + 60) * 1000);
    const state = await sessionStub(env, sessionId).ensure(user.id, userName, chat.id, title);
    if (state.status === 'open') {
      await index.extend(sessionId, furthestDeadline(state));
      return;
    }
    await index.finish(sessionId);
  }
  console.error('could not open a verification session', { chatId: chat.id, userId: user.id });
}

async function onMemberLeft(env: Env, chatId: number, userId: number): Promise<void> {
  const active = await indexStub(env, userId).active();
  if (!active) return;
  await sessionStub(env, active.sessionId).memberLeft(chatId);
}

/**
 * Handles one message in a group: the arrival notice, and anything a still-muted member
 * managed to send.
 *
 * Both need the bot to be able to SEE the message, and to delete it. Telegram withholds
 * ordinary group messages from a bot that is in privacy mode, so that mode is turned off
 * (see the README), and deleting needs the bot's `can_delete_messages` right.
 */
async function onGroupMessage(env: Env, msg: TgMessage): Promise<void> {
  const from = msg.from;
  // Only a member with a live verification can be muted, and that is almost nobody — so
  // this single lookup is the whole cost of the bot for an ordinary message.
  const muted = from !== undefined && !from.is_bot && (await isMuted(env, msg.chat.id, from.id));

  if (msg.new_chat_members?.length) {
    if (!(await gatedChat(env, msg.chat))) return;
    // Fallback for chats that do not deliver `chat_member` updates. `ensure` is
    // idempotent, so both paths firing for one join cannot double-mute or double-post.
    for (const member of msg.new_chat_members) await onMemberJoined(env, msg.chat, member);
    // "X joined the group": the trail of who arrived (and later, who was removed) is not
    // something the group needs to keep, and it is the first thing to clear for the
    // member who has just arrived.
    await tryCallTelegram(env, 'deleteMessage', {
      chat_id: msg.chat.id,
      message_id: msg.message_id,
    });
    return;
  }

  if (muted) {
    // Only reachable between joining and the mute landing: once the restriction is on,
    // the member cannot send anything for this to delete. Deliberately uncached — a cache
    // would keep deleting the messages of somebody who has just passed.
    await tryCallTelegram(env, 'deleteMessage', {
      chat_id: msg.chat.id,
      message_id: msg.message_id,
    });
  }
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
  const reply = (text: string) =>
    callTelegram(env, 'sendMessage', {
      chat_id: chatId,
      text,
      parse_mode: 'HTML',
      link_preview_options: { is_disabled: true },
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
  const attempt = Math.min(state.attempts + 1, MAX_ATTEMPTS);
  await reply(
    `Verify here — ${mmss(deadline - Date.now())} left, attempt ${attempt} of ${MAX_ATTEMPTS}:\n` +
      `${env.VERIFY_ORIGIN}/telegram?session=${state.sessionId}`,
  );
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

    // The service message that announces an arrival is also the only update carrying the
    // id needed to delete it, so it is handled before the general group path.
    if (msg.new_chat_members?.length) {
      await onGroupMessage(env, msg);
      return;
    }

    if (!msg.from || msg.from.is_bot) return;
    await onGroupMessage(env, msg);
  } catch (err) {
    console.error('update failed', update.update_id, err instanceof Error ? err.message : err);
  }
}
