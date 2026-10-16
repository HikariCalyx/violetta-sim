import { DurableObject } from 'cloudflare:workers';
import type { Env, RegisteredChat } from './env';

/**
 * Which chats gate their new members.
 *
 * Read on every join, so it must be strongly consistent — hence a Durable Object rather
 * than KV, which can serve a stale list for up to a minute: long enough to miss the join
 * that follows the promotion that put the bot in charge of a chat.
 */
export class ChatRegistry extends DurableObject<Env> {
  private async all(): Promise<RegisteredChat[]> {
    return (await this.ctx.storage.get<RegisteredChat[]>('chats')) ?? [];
  }

  private async write(chats: RegisteredChat[]): Promise<void> {
    await this.ctx.storage.put('chats', chats);
  }

  private async upsert(entry: RegisteredChat): Promise<RegisteredChat> {
    const chats = await this.all();
    const at = chats.findIndex((chat) => chat.chatId === entry.chatId);
    if (at >= 0) chats[at] = entry;
    else chats.push(entry);
    await this.write(chats);
    return entry;
  }

  async list(): Promise<RegisteredChat[]> {
    return this.all();
  }

  async get(chatId: number): Promise<RegisteredChat | null> {
    return (await this.all()).find((chat) => chat.chatId === chatId) ?? null;
  }

  /** Records that the bot can gate this chat, leaving any admin's choice intact. */
  async remember(chatId: number, title: string): Promise<RegisteredChat> {
    const known = await this.get(chatId);
    return this.upsert({
      chatId,
      title,
      checkedAt: Date.now(),
      disabledAt: known?.disabledAt ?? null,
    });
  }

  /** Switches gating on for this chat, clearing an admin's earlier `off`. */
  async enable(chatId: number, title: string): Promise<RegisteredChat> {
    return this.upsert({ chatId, title, checkedAt: Date.now(), disabledAt: null });
  }

  /** Switches gating off for this chat, remembering that the bot could still do it. */
  async disable(chatId: number, title: string): Promise<RegisteredChat> {
    const known = await this.get(chatId);
    return this.upsert({
      chatId,
      title,
      checkedAt: known?.checkedAt ?? Date.now(),
      disabledAt: Date.now(),
    });
  }

  /** Forgets the chat: the bot was removed from it, or lost a right it needs. */
  async forget(chatId: number): Promise<void> {
    await this.write((await this.all()).filter((chat) => chat.chatId !== chatId));
  }

  /**
   * The bot's own user id, remembered so a rights check never needs another `getMe`.
   */
  async botId(): Promise<number | null> {
    return (await this.ctx.storage.get<number>('botId')) ?? null;
  }

  async setBotId(botId: number): Promise<void> {
    await this.ctx.storage.put('botId', botId);
  }
}
