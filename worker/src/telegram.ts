import type { Env } from './env';

const API = 'https://api.telegram.org';

/** How long a new member has, in total, to finish the mini-game. */
export const MUTE_SECONDS = 10 * 60;

/**
 * One shared counter per user, not per chat: "finish once" means one game and three
 * tries in total, so joining a second protected chat mid-game does not refill it.
 */
export const MAX_ATTEMPTS = 3;

/**
 * Every send flag false. In a supergroup `can_send_messages: false` already covers the
 * media flags unless `use_independent_chat_permissions` is passed, but naming them costs
 * nothing and still holds if that flag is turned on later.
 */
export const MUTED_PERMISSIONS = {
  can_send_messages: false,
  can_send_audios: false,
  can_send_documents: false,
  can_send_photos: false,
  can_send_videos: false,
  can_send_video_notes: false,
  can_send_voice_notes: false,
  can_send_polls: false,
  can_send_other_messages: false,
  can_add_web_page_previews: false,
  can_change_info: false,
  can_invite_users: false,
  can_pin_messages: false,
  can_manage_topics: false,
};

/** Lifting a restriction: the member is back to the chat's own default permissions. */
export const FULL_PERMISSIONS = {
  can_send_messages: true,
  can_send_audios: true,
  can_send_documents: true,
  can_send_photos: true,
  can_send_videos: true,
  can_send_video_notes: true,
  can_send_voice_notes: true,
  can_send_polls: true,
  can_send_other_messages: true,
  can_add_web_page_previews: true,
  can_change_info: true,
  can_invite_users: true,
  can_pin_messages: true,
  can_manage_topics: true,
};

export interface TgUser {
  id: number;
  is_bot: boolean;
  first_name: string;
  last_name?: string;
  username?: string;
}

export interface TgChat {
  id: number;
  type: 'private' | 'group' | 'supergroup' | 'channel';
  title?: string;
  username?: string;
  is_forum?: boolean;
}

export interface TgChatMember {
  status: 'creator' | 'administrator' | 'member' | 'restricted' | 'left' | 'kicked';
  user: TgUser;
  is_member?: boolean;
  until_date?: number;
  // Telegram omits permission fields that are false.
  can_delete_messages?: boolean;
  can_restrict_members?: boolean;
}

export interface TgChatMemberUpdated {
  chat: TgChat;
  from: TgUser;
  date: number;
  old_chat_member: TgChatMember;
  new_chat_member: TgChatMember;
}

export interface TgMessage {
  message_id: number;
  from?: TgUser;
  chat: TgChat;
  date: number;
  text?: string;
  message_thread_id?: number;
  new_chat_members?: TgUser[];
}

export interface TgUpdate {
  update_id: number;
  message?: TgMessage;
  chat_member?: TgChatMemberUpdated;
  my_chat_member?: TgChatMemberUpdated;
}

export interface TgApiResponse<T> {
  ok: boolean;
  result?: T;
  description?: string;
  error_code?: number;
}

export async function callTelegram<T>(
  env: Env,
  method: string,
  payload: Record<string, unknown>,
): Promise<T> {
  const res = await fetch(`${API}/bot${env.BOT_TOKEN}/${method}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(payload),
  });
  const body = (await res.json()) as TgApiResponse<T>;
  if (!body.ok) {
    // No token in the message on purpose: Worker logs are retained and queryable.
    throw new Error(`telegram ${method} failed (${body.error_code}): ${body.description}`);
  }
  return body.result as T;
}

/**
 * For cleanup calls inside an alarm: one un-deletable notice or an already-gone member
 * must not abort the rest of the sweep.
 */
export async function tryCallTelegram(
  env: Env,
  method: string,
  payload: Record<string, unknown>,
): Promise<void> {
  try {
    await callTelegram(env, method, payload);
  } catch (err) {
    console.error(`telegram ${method} failed`, payload, err instanceof Error ? err.message : err);
  }
}

export function displayName(user: TgUser): string {
  return [user.first_name, user.last_name].filter(Boolean).join(' ');
}

export function escapeHtml(text: string): string {
  return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
}

/** HTML mention that also works for members without a username. */
export function mention(userId: number, name: string): string {
  return `<a href="tg://user?id=${userId}">${escapeHtml(name)}</a>`;
}

export function mmss(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  return `${Math.floor(total / 60)}:${String(total % 60).padStart(2, '0')}`;
}
