"""One-time interactive login that prints the StringSession secret.

Run this on a trusted machine, then store the printed value as the repository secret
``USERBOT_SESSION``. The session is a logged-in account: treat it like a password.
"""

from __future__ import annotations

import asyncio

from telethon import TelegramClient
from telethon.sessions import StringSession


async def main() -> None:
    api_id = int(input("API ID: ").strip())
    api_hash = input("API hash: ").strip()

    async with TelegramClient(StringSession(), api_id, api_hash) as client:
        # The client asks for the phone number, the login code and (if enabled) the
        # two-factor password on the terminal, then keeps the authorised session.
        me = await client.get_me()
        print(f"\nLogged in as {me.first_name} (id {me.id}).\n")
        print("USERBOT_SESSION (store this as a repository secret):")
        print(client.session.save())


if __name__ == "__main__":
    asyncio.run(main())
