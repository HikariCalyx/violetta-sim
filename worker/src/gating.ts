import { REGISTRY_NAME, newSessionId, userIndexName, type Env } from './env';
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

export async function onMemberJoined(env: Env, chat: TgChat, user: TgUser): Promise<void> {
  if (user.is_bot) return;

  const registered = await registryStub(env).get(chat.id);
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
    if (!(await registryStub(env).get(msg.chat.id))) return;
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
  // `my_chat_member` is only ever about this bot, so no getMe is needed here.
  const chatId = update.chat.id;
  const status = update.new_chat_member.status;
  const registry = registryStub(env);

  if (status === 'left' || status === 'kicked') {
    if (await registry.get(chatId)) {
      await registry.unregister(chatId);
      console.warn('bot removed from a registered chat; verification disabled', { chatId });
    }
    return;
  }
  if (status === 'member' && (await registry.get(chatId))) {
    console.warn('bot is no longer an admin here: muting will fail until it is promoted', { chatId });
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

/** `/verify_here on|off` — how a chat opts in. Admin-only. */
async function onVerifyHere(env: Env, msg: TgMessage): Promise<void> {
  const from = msg.from;
  if (!from) return;
  const chatId = msg.chat.id;
  const registry = registryStub(env);
  const arg = (msg.text ?? '').split(/\s+/)[1]?.toLowerCase();
  const reply = (text: string) =>
    callTelegram(env, 'sendMessage', {
      chat_id: chatId,
      text,
      ...(msg.message_thread_id ? { message_thread_id: msg.message_thread_id } : {}),
    });

  const member = await callTelegram<TgChatMember>(env, 'getChatMember', {
    chat_id: chatId,
    user_id: from.id,
  });
  if (member.status !== 'creator' && member.status !== 'administrator') {
    await reply('Only a chat admin can change verification for this chat.');
    return;
  }

  if (arg !== 'on' && arg !== 'off') {
    await reply(
      (await registry.get(chatId))
        ? 'Verification is ON for this chat. An admin can turn it off with /verify_here off.'
        : 'Verification is OFF for this chat. An admin can turn it on with /verify_here on.',
    );
    return;
  }

  if (arg === 'off') {
    await registry.unregister(chatId);
    await reply(
      'Verification is OFF for this chat. Members muted earlier stay muted — lift those by hand ' +
        'if this was a mistake.',
    );
    return;
  }

  // Refuse to switch on unless the bot can do both jobs. In the Bot API there is no
  // separate "Ban users" permission: muting and kicking are both `can_restrict_members`,
  // which is what the clients label "Ban users". Delete rights clear the notice again.
  const me = await callTelegram<TgUser>(env, 'getMe', {});
  const self = await callTelegram<TgChatMember>(env, 'getChatMember', {
    chat_id: chatId,
    user_id: me.id,
  });
  // Telegram omits false permission fields, so these must be compared to `true`.
  const canMute = self.can_restrict_members === true;
  const canDelete = self.can_delete_messages === true;
  if (self.status !== 'administrator' || !canMute || !canDelete) {
    const missing = [
      self.status !== 'administrator' && 'be an admin',
      !canMute && 'have "Ban users"',
      !canDelete && 'have "Delete messages"',
    ]
      .filter(Boolean)
      .join(' and ');
    await reply(`I need to ${missing} before verification can be turned on.`);
    return;
  }

  await registry.register(chatId, msg.chat.title ?? 'the group');
  await reply(
    `Verification is ON for this chat: new members are muted for ${MUTE_SECONDS / 60} minutes and must ` +
      `finish the mini-game in ${env.BOT_USERNAME}'s PM.`,
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
    if (msg.text?.startsWith('/verify_here')) {
      await onVerifyHere(env, msg);
      return;
    }
    await onGroupMessage(env, msg);
  } catch (err) {
    console.error('update failed', update.update_id, err instanceof Error ? err.message : err);
  }
}
