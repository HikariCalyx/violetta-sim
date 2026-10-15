import { DurableObject } from 'cloudflare:workers';
import type { Env, RegisteredChat } from './env';

/**
 * Which chats gate their members. Written by `/verify_here on|off` from a chat admin
 * and read on every join, so it must be strongly consistent — hence a Durable Object
 * rather than KV (which can serve a stale list for up to a minute).
 */
export class ChatRegistry extends DurableObject<Env> {
  private async all(): Promise<RegisteredChat[]> {
    return (await this.ctx.storage.get<RegisteredChat[]>('chats')) ?? [];
  }

  async list(): Promise<RegisteredChat[]> {
    return this.all();
  }

  async get(chatId: number): Promise<RegisteredChat | null> {
    return (await this.all()).find((chat) => chat.chatId === chatId) ?? null;
  }

  async has(chatId: number): Promise<boolean> {
    return (await this.get(chatId)) !== null;
  }

  async register(chatId: number, title: string): Promise<RegisteredChat> {
    const chats = await this.all();
    const entry: RegisteredChat = { chatId, title, registeredAt: Date.now() };
    const at = chats.findIndex((chat) => chat.chatId === chatId);
    if (at >= 0) chats[at] = entry;
    else chats.push(entry);
    await this.ctx.storage.put('chats', chats);
    return entry;
  }

  async unregister(chatId: number): Promise<void> {
    const chats = (await this.all()).filter((chat) => chat.chatId !== chatId);
    await this.ctx.storage.put('chats', chats);
  }
}
