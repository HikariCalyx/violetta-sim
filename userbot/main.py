"""The PM verification userbot.

A user account (not a Bot API bot) watches private messages. A stranger who has never
talked to this account before and shares fewer than three groups gets a message with the
mini-game link, is blocked, and has their chat archived. Passing the game makes the
Worker queue a cleanup: this program deletes its own message, unblocks the stranger and
unarchives the chat. Failing leaves everything in place.

Run it from the repository root:

    USERBOT_API_ID=... API_HASH_SECRET=... USERBOT_SESSION=... python userbot/main.py

The GitHub Actions workflow does exactly that on a schedule, because an MTProto user
account cannot be woken by a webhook.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import logging
import os
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

import aiohttp
from telethon import TelegramClient, events, functions, types
from telethon.errors import FloodWaitError, RPCError
from telethon.sessions import StringSession

try:
    from .languages import preferred_language, verification_message
except ImportError:  # running as `python userbot/main.py`
    from languages import preferred_language, verification_message

LOG = logging.getLogger("violetta-userbot")

# Telegram's archive is folder 1; 0 is the main chat list.
ARCHIVE_FOLDER = 1
MAIN_FOLDER = 0

# How many groups in common still count as "a stranger".
MAX_COMMON_GROUPS = 3

# Where the photo that opens the verification message lives. It is the same artwork the
# game uses for Violetta, embedded in the repository rather than fetched at runtime.
DEFAULT_PHOTO = Path(__file__).resolve().parent.parent / "Resources" / "lieDetector.npcSpecial.stand.0.png"


def _optional_int(name: str, default: int) -> int:
    raw = os.environ.get(name, "").strip()
    if not raw:
        return default
    try:
        return int(raw)
    except ValueError as exc:
        raise SystemExit(f"{name} must be an integer, got {raw!r}") from exc


MAX_RUNTIME_SECONDS = _optional_int("MAX_RUNTIME_SECONDS", 0)
TRIGGER_MAX_AGE_SECONDS = _optional_int("TRIGGER_MAX_AGE_SECONDS", 30 * 60)
PHOTO = Path(os.environ.get("USERBOT_PHOTO", str(DEFAULT_PHOTO))).expanduser()


def _required(name: str) -> str:
    value = os.environ.get(name, "").strip()
    if not value:
        raise SystemExit(f"{name} is not set")
    return value


class WorkerError(RuntimeError):
    """The Worker refused a userbot API call."""


class Worker:
    """The small slice of the Worker the userbot needs.

    The bearer is derived from the api_hash rather than sent raw. The Worker derives the
    same value, so the account's real credential never crosses the network.
    """

    def __init__(self, http: aiohttp.ClientSession, origin: str, api_hash: str) -> None:
        self.http = http
        self.origin = origin.rstrip("/")
        self.token = hashlib.sha256(f"violetta-userbot:{api_hash}".encode()).hexdigest()

    async def _call(self, method: str, path: str, payload: dict[str, Any] | None = None) -> Any:
        headers = {"authorization": f"Bearer {self.token}"}
        timeout = aiohttp.ClientTimeout(total=20)
        async with self.http.request(
            method,
            f"{self.origin}{path}",
            json=payload,
            headers=headers,
            timeout=timeout,
        ) as response:
            text = await response.text()
            if response.status >= 400:
                raise WorkerError(f"{method} {path} -> HTTP {response.status}: {text[:300]}")
            try:
                return json.loads(text)
            except ValueError:
                return None

    async def create_session(self, user_id: int, name: str) -> dict[str, Any]:
        result = await self._call("POST", "/api/userbot/session", {"userId": user_id, "userName": name})
        if not isinstance(result, dict):
            raise WorkerError(f"unexpected create-session answer: {result!r}")
        if result.get("verified"):
            return result
        if not result.get("sessionId"):
            raise WorkerError(f"unexpected create-session answer: {result!r}")
        return result

    async def sent(self, session_id: str, message_id: int) -> None:
        await self._call("POST", f"/api/userbot/session/{session_id}/sent", {"messageId": message_id})

    async def abort(self, session_id: str) -> None:
        await self._call("POST", f"/api/userbot/session/{session_id}/abort", {})

    async def actions(self) -> list[dict[str, Any]]:
        result = await self._call("GET", "/api/userbot/actions")
        actions = result.get("actions") if isinstance(result, dict) else None
        return actions if isinstance(actions, list) else []

    async def ack(self, action_id: str) -> None:
        await self._call("POST", f"/api/userbot/actions/{action_id}", {})

    async def last_seen(self) -> int | None:
        result = await self._call("GET", "/api/userbot/heartbeat")
        value = result.get("lastSeen") if isinstance(result, dict) else None
        return value if isinstance(value, int) else None

    async def heartbeat(self) -> None:
        await self._call("POST", "/api/userbot/heartbeat", {})


async def _has_history_before(client: TelegramClient, user: Any, message_id: int) -> bool:
    """Whether anything at all was exchanged with this user before ``message_id``.

    The message being handled is already in the history, so it is excluded. Any earlier
    message — incoming or outgoing — means this is not a first contact.
    """
    if message_id <= 1:
        return False
    # Telethon's max_id is EXCLUSIVE (messages with an id >= max_id are excluded), so
    # passing the current id includes the message immediately before it. `id - 1` would
    # hide exactly that message and make a one-message-old contact look like a stranger.
    previous = await client.get_messages(user, limit=1, max_id=message_id)
    return len(previous) > 0


async def _resolve_user(client: TelegramClient, user_id: int) -> Any | None:
    """Find a user entity by id, even after a restart.

    A StringSession keeps the auth key but not Telethon's entity cache, and unblocking
    needs an access hash. The blocked list is the reliable place to look, because every
    user this program cleans up was blocked first.
    """
    offset = 0
    while True:
        try:
            result = await client(functions.contacts.GetBlockedRequest(offset=offset, limit=100))
        except RPCError as exc:
            LOG.warning("could not read the blocked list: %s", exc)
            break
        for user in result.users:
            if user.id == user_id:
                return user
        if len(result.blocked) < 100:
            break
        offset += len(result.blocked)

    try:
        return await client.get_entity(user_id)
    except Exception:  # noqa: BLE001 - a missing cache entry is expected here
        pass

    # Main and archive: the block or the archive call may have failed, so the dialog can
    # be in either folder.
    for folder in (0, ARCHIVE_FOLDER):
        try:
            async for dialog in client.iter_dialogs(folder=folder):
                if getattr(dialog.entity, "id", None) == user_id:
                    return dialog.entity
        except Exception as exc:  # noqa: BLE001
            LOG.warning("could not scan folder %d for user %s: %s", folder, user_id, exc)
    return None


def _already_gone(exc: Exception) -> bool:
    """Errors that mean the cleanup goal is already true."""
    name = exc.__class__.__name__
    return any(
        marker in name
        for marker in (
            "MessageIdInvalid",
            "MessageDeleteForbidden",
            "UserNotMutualContact",
            "UserNotParticipant",
            "PeerIdInvalid",
        )
    )


async def _perform_action(
    client: TelegramClient,
    worker: Worker,
    action: dict[str, Any],
    failures: dict[str, int],
) -> None:
    action_id = str(action.get("id") or "")
    try:
        user_id = int(action.get("userId"))
    except (TypeError, ValueError):
        LOG.warning("dropping an action without a user id: %s", action)
        if action_id:
            await worker.ack(action_id)
        return

    user = await _resolve_user(client, user_id)
    if user is None:
        # Retrying cannot invent an access hash; leaving the action queued would only
        # make the userbot log the same warning forever.
        LOG.warning("could not resolve user %s; dropping the cleanup", user_id)
        if action_id:
            await worker.ack(action_id)
        return

    problems: list[str] = []

    message_id = action.get("messageId")
    if isinstance(message_id, int) and message_id > 0:
        try:
            await client.delete_messages(user, [message_id], revoke=True)
        except Exception as exc:  # noqa: BLE001
            if not _already_gone(exc):
                problems.append(f"delete: {exc}")

    try:
        await client(functions.contacts.UnblockRequest(id=user))
    except Exception as exc:  # noqa: BLE001
        if not _already_gone(exc):
            problems.append(f"unblock: {exc}")

    try:
        await client.edit_folder(user, MAIN_FOLDER)
    except Exception as exc:  # noqa: BLE001
        problems.append(f"unarchive: {exc}")

    if not problems:
        if action_id:
            await worker.ack(action_id)
        failures.pop(action_id, None)
        LOG.info("verification passed: deleted the link, unblocked and unarchived %s (%s)", user_id, action_id)
        return

    count = failures.get(action_id, 0) + 1
    failures[action_id] = count
    LOG.warning("cleanup for %s failed (attempt %d): %s", user_id, count, "; ".join(problems))
    if count >= 20:
        # Roughly a minute of retries. A permanently broken peer must not block every
        # later cleanup behind it.
        if action_id:
            await worker.ack(action_id)
        failures.pop(action_id, None)
        LOG.error("giving up on the cleanup for %s (%s)", user_id, action_id)


async def _poll_actions(client: TelegramClient, worker: Worker) -> None:
    failures: dict[str, int] = {}
    while True:
        try:
            for action in await worker.actions():
                await _perform_action(client, worker, action, failures)
        except asyncio.CancelledError:
            raise
        except Exception:  # noqa: BLE001
            LOG.exception("the action poll failed")
        await asyncio.sleep(3)


async def _verify(client: TelegramClient, worker: Worker, sender: Any) -> None:
    name = " ".join(part for part in (sender.first_name, sender.last_name) if part) or str(sender.id)
    try:
        created = await worker.create_session(sender.id, name)
    except Exception:  # noqa: BLE001
        LOG.exception("could not create a session for %s", sender.id)
        return

    if created.get("verified"):
        LOG.info("user %s already passed a verification; not gating again", sender.id)
        return

    session_id = str(created["sessionId"])
    if not created.get("created") and created.get("messageId") is not None:
        # Another message in the same burst already started this verification, or a
        # previous run did. Never send a second link.
        LOG.info("user %s already has a verification (%s)", sender.id, session_id)
        return
    # `created` is false with no message id only when the previous run created the
    # session and died before recording its send; finishing the job from here is safe.

    link = f"{worker.origin}/telegram?session={session_id}"
    # Telegram puts the recipient's own client language on the User object. A member
    # who never chose one has `lang_code` unset, and gets the English wording.
    lang_code = getattr(sender, "lang_code", None)
    sent = None
    try:
        sent = await client.send_file(
            sender,
            str(PHOTO),
            caption=verification_message(lang_code, link),
            parse_mode=None,
        )
        await worker.sent(session_id, sent.id)
    except Exception:  # noqa: BLE001
        LOG.exception("could not send the verification message to %s", sender.id)
        try:
            await worker.abort(session_id)
        except Exception:  # noqa: BLE001
            LOG.exception("could not abort the session %s", session_id)
        if sent is not None:
            try:
                await client.delete_messages(sender, [sent.id], revoke=True)
            except Exception:  # noqa: BLE001
                LOG.exception("could not remove a half-sent verification message")
        return

    for attempt in range(3):
        try:
            await client(functions.contacts.BlockRequest(id=sender))
            break
        except FloodWaitError as exc:
            await asyncio.sleep(exc.seconds + 1)
        except Exception:  # noqa: BLE001
            LOG.exception("could not block user %s (attempt %d)", sender.id, attempt + 1)
            await asyncio.sleep(2**attempt)
    else:
        LOG.error("user %s is still not blocked; the link remains valid", sender.id)

    try:
        await client.edit_folder(sender, ARCHIVE_FOLDER)
    except Exception:  # noqa: BLE001
        LOG.exception("could not archive the chat with user %s", sender.id)

    LOG.info(
        "verification sent to %s (%s, %s)",
        sender.id,
        session_id,
        preferred_language(lang_code),
    )


async def _process_private_message(
    client: TelegramClient,
    worker: Worker,
    message: Any,
    sender: Any,
) -> None:
    if message.out or message.action is not None:
        return
    if not isinstance(sender, types.User) or sender.bot or sender.is_self:
        return

    # A runner that was offline can replay old updates on reconnect. Verifying somebody
    # for a message from before the window is worse than skipping it.
    if message.date is not None:
        age = (datetime.now(timezone.utc) - message.date).total_seconds()
        if age > TRIGGER_MAX_AGE_SECONDS:
            return

    user_id = sender.id

    if await _has_history_before(client, sender, message.id):
        return

    try:
        # The raw request rather than a client helper: this is the call the gate is
        # defined by, and it names the limit explicitly.
        common = await client(
            functions.messages.GetCommonChatsRequest(user_id=sender, max_id=0, limit=100)
        )
    except RPCError as exc:
        # Without this answer the gate cannot be decided. Skipping is the safe side:
        # it never blocks somebody who should have been left alone.
        LOG.warning("could not count the chats in common with %s: %s", user_id, exc)
        return
    if len(common.chats) >= MAX_COMMON_GROUPS:
        return

    await _verify(client, worker, sender)


async def _sweep_recent(
    client: TelegramClient,
    worker: Worker,
    since: int | None,
    inflight: set[int],
) -> None:
    """Handle private messages that arrived between two GitHub Actions runs.

    A StringSession keeps the auth key but not the account's update cursor, and a fresh
    runner has no local state at all. The update stream alone would therefore start at
    "now" and miss anything sent during the hand-off. The dialog list is one request, and
    only a dialog whose latest message is a recent incoming one costs a small history
    read; the oldest message of that window is what decides "first contact".

    `since` is the previous run's last heartbeat, so a user who passed before the gap is
    not examined again merely because their old message is still the latest one.
    """
    now = datetime.now(timezone.utc)
    if since is None:
        cutoff = now - timedelta(seconds=TRIGGER_MAX_AGE_SECONDS)
    else:
        # Two minutes of slack covers the heartbeat interval and clock differences.
        cutoff = datetime.fromtimestamp(since / 1000, tz=timezone.utc) - timedelta(minutes=2)
    handled = 0
    async for dialog in client.iter_dialogs(limit=100):
        message = dialog.message
        entity = dialog.entity
        if message is None or message.out or message.action is not None:
            continue
        if not isinstance(entity, types.User) or entity.bot or entity.is_self:
            continue
        if message.date is None or message.date < cutoff:
            continue

        # The update stream is already live by the time this runs, so a message that
        # arrived during the hand-off may also reach the normal handler. One user is
        # handled once at a time, exactly as the handler does for a burst.
        user_id = entity.id
        if user_id in inflight:
            continue
        inflight.add(user_id)
        try:
            # One message is not enough: a stranger can send two in a burst, and the
            # newer one would look like a conversation that already existed. The oldest
            # recent message decides "first contact", so handle them in id order.
            try:
                history = await client.get_messages(entity, limit=100)
            except Exception:  # noqa: BLE001 - one bad dialog must not stop the sweep
                LOG.exception("could not read the recent messages of a dialog")
                continue
            recent = sorted(
                (
                    candidate
                    for candidate in (history or [])
                    if not candidate.out
                    and candidate.action is None
                    and candidate.date is not None
                    and candidate.date >= cutoff
                ),
                key=lambda candidate: candidate.id,
            )
            for candidate in recent:
                handled += 1
                try:
                    await _process_private_message(client, worker, candidate, entity)
                except Exception:  # noqa: BLE001
                    LOG.exception("could not handle a message from the startup sweep")
        finally:
            inflight.discard(user_id)
    if handled:
        LOG.info("startup sweep examined %d recent private message(s)", handled)


async def _heartbeat_loop(worker: Worker) -> None:
    while True:
        try:
            await worker.heartbeat()
        except asyncio.CancelledError:
            raise
        except Exception:  # noqa: BLE001
            LOG.exception("could not send the userbot heartbeat")
        await asyncio.sleep(60)


async def _stop_after(client: TelegramClient, seconds: int) -> None:
    await asyncio.sleep(seconds)
    LOG.info("runtime budget reached; the next scheduled run takes over")
    await client.disconnect()


async def run() -> None:
    api_id = int(_required("USERBOT_API_ID"))
    api_hash = _required("API_HASH_SECRET")
    session = _required("USERBOT_SESSION")
    # `or` rather than a get() default: an empty repository variable would otherwise
    # turn every URL into a relative path.
    origin = (os.environ.get("WORKER_ORIGIN") or "https://violetta.trill.workers.dev").strip()

    if not PHOTO.is_file():
        raise SystemExit(f"verification artwork not found: {PHOTO}")

    client = TelegramClient(
        StringSession(session),
        api_id,
        api_hash,
        catch_up=True,
        flood_sleep_threshold=60,
        device_model="Violetta Gate",
        system_version="1.0",
        app_version="1.0",
    )

    inflight: set[int] = set()

    async def handler(event: events.NewMessage.Event) -> None:
        if not event.is_private:
            return
        sender = await event.get_sender()
        user_id = getattr(sender, "id", None)
        if user_id is None:
            return
        # Two messages in one burst arrive as two tasks; the history check makes the
        # second one a no-op, but this keeps them from racing the same network calls.
        if user_id in inflight:
            return
        inflight.add(user_id)
        try:
            await _process_private_message(client, worker, event.message, sender)
        except Exception:  # noqa: BLE001 - one bad update must not stop the userbot
            LOG.exception("could not handle a private message from %s", user_id)
        finally:
            inflight.discard(user_id)

    async with aiohttp.ClientSession() as http:
        worker = Worker(http, origin, api_hash)
        # Registered only once `worker` exists: a caught-up update can otherwise reach
        # the handler between connect() and the assignment.
        client.add_event_handler(handler, events.NewMessage(incoming=True))

        await client.connect()
        if not await client.is_user_authorized():
            raise SystemExit(
                "USERBOT_SESSION is not authorised. Run `python userbot/login.py` once and "
                "store the printed value as the USERBOT_SESSION repository secret."
            )

        me = await client.get_me()
        LOG.info("userbot running as %s (id %s) against %s", me.first_name, me.id, origin)

        # Before the update stream is trusted: a run that starts after a hand-off gap
        # must still see the messages that arrived while nothing was connected.
        try:
            await _sweep_recent(client, worker, await worker.last_seen(), inflight)
        except Exception:  # noqa: BLE001 - a failed sweep must not stop the run
            LOG.exception("the startup sweep failed")

        tasks = [
            asyncio.create_task(_poll_actions(client, worker)),
            asyncio.create_task(_heartbeat_loop(worker)),
        ]
        if MAX_RUNTIME_SECONDS > 0:
            tasks.append(asyncio.create_task(_stop_after(client, MAX_RUNTIME_SECONDS)))
        try:
            await client.run_until_disconnected()
        finally:
            for task in tasks:
                task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)


def main() -> None:
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
    )
    try:
        asyncio.run(run())
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
