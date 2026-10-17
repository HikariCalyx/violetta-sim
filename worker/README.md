# violetta

A Cloudflare Worker that gates the members of a Telegram group behind the mini-game in
this repository. (`violetta` is the Worker's name in `wrangler.jsonc`, which is also what
the Cloudflare dashboard lists and what an API token scoped to one Worker would use.)

A new member is muted and can only talk after finishing the game in the bot's PM. A normal
verification allows 10 minutes in total and three attempts; running out of time, or failing
them all, removes the member from the chat — and the removal lifts the mute as well as the
ban, so nothing is left behind that would keep them quiet or list them as restricted.

The arrival notice greets them by name with a **Start** button into the bot, and the
announcement that they joined is deleted again. Anything the member manages to send in
the moment before the mute lands is recorded and deleted too, and so is any answer a
guest bot posts on their behalf while they are muted — a muted member can still get a
message in by summoning one. A member caught this way plays the mini-game in **chaos
mode**, as does anyone who joins the same chat more than three times within 35 minutes.

Chaos mode shuffles for 7 seconds instead of 5, moves the four through three shuffle
passes instead of one, doubles the watermark figures and raises them to 90, cuts the
answer window to 3 seconds, and writes the answers with the second digit set. It also
gives **15 minutes and ten attempts** instead of 10 and three, and a member who fails it
**stays banned** — they remain in the chat's *Removed users* list instead of being
unbanned to try again.

**No chat has to be enrolled.** The bot gates every chat where it is an administrator with
**Ban users** and **Delete messages** — being promoted is the whole opt-in. The first join
in a chat it has not seen before is what confirms the rights, because a promotion that
happened before the Worker existed sends no update of its own. Until then that chat is a
complete no-op, which is worth knowing before debugging a silent bot.

There is no command to run and nothing to enrol: being an admin *is* the opt-in, and taking
those two rights away — or removing the bot — is what stops it. That also means the bot
gates **every** chat where it holds them, so do not hand it admin rights somewhere it
should keep quiet.

## The PM userbot

`userbot/` is a separate MTProto **user account**, because the Bot API has no way to list
the groups you share with someone, block a user or archive a chat. It gates private
messages:

- A message from somebody who has **never talked to this account before** and shares
  **fewer than three groups** triggers verification. "Never talked" means no message at
  all before the one being handled (Telethon's `max_id` is exclusive, so the check passes
  the current message id). The userbot sends the Violetta artwork with the invitation
  text, creates a session, **blocks** the sender and **archives** the chat.
- The session is the normal mini-game with **10 minutes and five tries**. Scoring stays
  in the same `VerificationSession` object as the group gate, so the page cannot report
  its own outcome.
- Passing makes the Worker queue a cleanup in `UserbotInbox`: the userbot deletes its own
  message (for both sides), unblocks the sender and moves the chat out of the archive.
  The queue survives the userbot being offline; it drains every three seconds, and the
  pass is recorded in the user's index so a restart (or a cleared history) cannot gate
  them a second time.
- Each run sends a heartbeat. The next run sweeps only the private dialogs whose latest
  message arrived after that heartbeat, so a message sent during the hand-off between two
  Actions runs is still caught.
- Running out of time or tries leaves the sender blocked. The verification link then
  says exactly:

  > **Verify failed**
  >
  > You have failed verification.

A PM verification is always normal mode, even if the same session also covers a group
that was flagged as chaotic.

### Running the userbot

A user account cannot receive a webhook, so `.github/workflows/userbot.yml` runs the
process for 5h45m per job and keeps an hourly schedule queued; the `concurrency` group
allows only one account session to be connected at a time, and the next run takes over
when the current one ends.

One-time setup:

1. Install the dependencies and create a `StringSession` on a trusted machine:
   ```sh
   pip install -r userbot/requirements.txt
   python userbot/login.py
   ```
   The printed value is a logged-in account. Treat it like a password; never commit it.
2. In **Settings → Secrets and variables → Actions**:

   | Name | Kind | What it is |
   | --- | --- | --- |
   | `USERBOT_API_ID` | variable | the `api_id` from https://my.telegram.org |
   | `API_HASH_SECRET` | secret | the `api_hash` from https://my.telegram.org; also pushed to the Worker as `USERBOT_API_HASH` |
   | `USERBOT_SESSION` | secret | the value printed by `userbot/login.py` |
   | `WORKER_ORIGIN` | optional variable | defaults to `https://violetta.trill.workers.dev` |

3. Deploy the Worker once (the deploy workflow pushes `USERBOT_API_HASH`), then run
   **Run the userbot** once from the Actions tab to confirm it connects.

Notes:

- Use a **dedicated account**, not your main one. A StringSession can read and send
  everything the account can.
- The userbot and the Worker derive the same bearer token from `API_HASH_SECRET`:
  `sha256("violetta-userbot:" + api_hash)`. The api_hash itself never crosses the wire.
- Scheduled workflows are disabled after 60 days without repository activity, and a
  private repository pays Actions minutes for a nearly continuous job. GitHub also runs
  `schedule` only from the **default branch**, so on another branch use **Run workflow**
  (or merge first).
- The account's common-group count and message history are read at the moment a PM first
  arrives; deleting the old history does not make an old contact a stranger again.

## How it fits together

| Piece | Where | Why |
| --- | --- | --- |
| Webhook + API routes | `src/index.ts` | One origin for the game page and the API, so there is no CORS |
| Telegram calls, permissions | `src/telegram.ts` | All Bot API traffic; every failure carries no token |
| Update dispatch, `/start` | `src/gating.ts` | The only place that decides "this member must be gated" |
| `ChatRegistry` DO | `src/registry.ts` | Which chats the bot is gating. Strongly consistent on purpose |
| `UserIndex` DO | `src/user-index.ts` | Which session is open for a user, so a UUID in the URL is enough |
| `VerificationSession` DO | `src/session.ts` | The state machine, the countdown alarm, the round scoring |
| `dist/` | `../dist` | The trunk-built game, served by the same Worker |

One verification covers **every** protected chat the member has joined: finishing it
unmutes them everywhere at once, while each chat keeps its own 10-minute deadline. The
attempt counter is shared — one game, three tries.

## Setup

1. Create the bot with @BotFather and copy its token. Also send it `/setprivacy` →
   **Disable**: Telegram withholds ordinary group messages from a bot that is in privacy
   mode, and the bot cannot delete what it never sees. In the @BotFather Mini App, also
   enable **Bot-to-Bot Communication Mode**, which is what lets the bot see (and delete)
   the answers a guest bot posts on a member's behalf.
2. `cd worker && npm install`
3. Register the secrets (they must never land in `wrangler.jsonc`). **The GitHub Actions
   workflow does this for you** — see below — so this is only for a hand deploy:
   ```sh
   npx wrangler secret put BOT_TOKEN
   npx wrangler secret put WEBHOOK_SECRET   # any random string, 1-256 chars of A-Za-z0-9_-
   ```
4. Deploy. The assets binding needs the game built first — `dist/` is git-ignored. Push
   and let the workflow do it (below), or run it by hand from a machine with Node >= 22,
   since wrangler refuses to start on anything older:
   ```sh
   (cd .. && trunk build --release) && npx wrangler deploy
   ```
5. Point Telegram at the Worker. `allowed_updates` matters: `chat_member` is **not**
   delivered unless it is named here. The workflow does this after every deploy too, in
   which case this is the hand-deploy equivalent:
   ```sh
   curl -sS "https://api.telegram.org/bot<TOKEN>/setWebhook" \
     -H 'content-type: application/json' \
     -d '{"url":"https://<worker-host>/telegram/webhook",
          "secret_token":"<WEBHOOK_SECRET>",
          "allowed_updates":["message","chat_member","my_chat_member"]}'
   ```
6. Add the bot to the group as an admin with **Ban users** and **Delete messages**. That is
   all: the next join is muted and answered, and the chat is remembered from then on. The
   log says `gating this chat: the bot has the rights it needs` the first time it takes
   charge of one.

`BOT_USERNAME` and `VERIFY_ORIGIN` in `wrangler.jsonc` must match the real bot and the
domain the Worker answers on. A Workers custom domain has to be a zone on the same
Cloudflare account — uncomment the `routes` block once that is true.

## Deploying from GitHub Actions

This is the intended path: the runner provides Node 22, so a developer machine with an
older Node never has to run wrangler at all. `.github/workflows/deploy-worker.yml` builds
the wasm bundle, checks its startup, typechecks the Worker and deploys — on every push to
`main` or `ghpages-src` that touches the bundle or `worker/`, plus `workflow_dispatch`.

| Repository secret | Where it comes from |
| --- | --- |
| `CLOUDFLARE_API_TOKEN` | an API token from the "Edit Cloudflare Workers" template (covers Workers Scripts and Workers Routes, which a custom domain needs) |
| `CLOUDFLARE_ACCOUNT_ID` | the account id in the Cloudflare dashboard sidebar |
| `BOT_TOKEN` | @BotFather |
| `WEBHOOK_SECRET` | any random string, 1-256 characters of `A-Za-z0-9_-` |
| `API_HASH_SECRET` | optional; the userbot's `api_hash`, also pushed as `USERBOT_API_HASH` |

After the deploy the workflow pushes `BOT_TOKEN` and `WEBHOOK_SECRET` onto the Worker with
`wrangler secret put`, cross-checks the deployed configuration through `/api/health` (it
fails if a secret did not land, or if the workflow's `VERIFY_ORIGIN` disagrees with
`wrangler.jsonc`), and then registers the webhook — so nothing has to be set by hand. Both
are idempotent, but note that one bot can only have one webhook: a second Worker deployed
for the same bot would take it over.

`BOT_TOKEN` and `WEBHOOK_SECRET` are scoped to those three steps rather than the whole job,
so they never enter the environment of `npm ci` and its install scripts.

The workflow and the Pages one both build the bundle, with different base paths (`/` here,
`/violetta-sim/` on Pages): they are independent deployments of the same game.

## HTTP API

| Route | Notes |
| --- | --- |
| `POST /telegram/webhook` | Telegram only. Requires the `X-Telegram-Bot-Api-Secret-Token` header; answers `ok` before doing the work |
| `GET /telegram?session=<uuid>` | The game page (the same bundle as `/`) |
| `GET /api/session/<uuid>/state` | `status`, `deadline`, `attemptsLeft`, the shuffle, `serverNow`, and `chaos` (the member was caught posting) |
| `POST /api/session/<uuid>/pick` | `{ roundIndex, slot, elapsedMs }` — the only write the browser makes. A `slot` of 0 means "ran out of time": it scores as a miss instead of leaving the attempt open |
| `POST /api/userbot/session` | Userbot only. `{ userId, userName }` → `{ sessionId, created }`; `created` says whether it still has to send the link |
| `POST /api/userbot/session/<uuid>/sent` | Userbot only. `{ messageId }`, so a pass can delete that exact message |
| `POST /api/userbot/session/<uuid>/abort` | Userbot only. Drops the PM side again when the send failed |
| `GET /api/userbot/actions` | Userbot only. Pending pass cleanups (`userId`, `messageId`, `sessionId`) |
| `POST /api/userbot/actions/<uuid>` | Userbot only. Acknowledges one cleanup after it ran |
| `GET \| POST /api/userbot/heartbeat` | Userbot only. Reports or reads the last moment a run was alive; the next run sweeps from it |
| `GET /api/health` | Liveness |

`state` returns the session's outcome after it closes, so the page can say "you were
removed" instead of breaking. Only an unknown session id is a 404.

## Known limitations

- **The answer is visible to the player.** The shuffle comes from the server, so the page
  cannot fake a win by POSTing `success: true`, but the special character is drawn with
  open eyes — moving the shuffle server-side raises the bar to "you must actually play".
- **The session UUID is a bearer token.** Anyone the member forwards the link to can play
  on their behalf. It is unguessable, and PM-only, which is the same trust level as the
  member handing over their phone.
- **Reloading mid-attempt costs a try.** The round counter lives in the DO, so a page that
  comes back in the middle of an attempt cannot replay the rounds it already scored — and
  the reveal that told the player which character to follow is gone. The page therefore
  reports the interrupted round as a miss and starts a fresh attempt with a fresh shuffle.
- **Deleting a member's messages needs the bot to see them.** Telegram withholds ordinary
  group messages from a bot in privacy mode, so with privacy on the bot still mutes and
  posts its notice but cannot clear anything sent in the moment before the mute landed.
  `/setprivacy` → Disable is what fixes that. Deleting needs `can_delete_messages` too.
- **Capturing the pre-mute window is a best-effort race.** Telegram processes webhook
  updates concurrently, so the join handler marks the member in their `UserIndex` before
  it does any network call; a message update that overtakes even that marker cannot be
  attributed to the join. Guest-bot answers are only visible with **Bot-to-Bot
  Communication Mode** enabled, so without it they are neither seen nor deleted.
- **Chaos mode is decided from what was captured or how often they joined.** A message
  captured before the mute, a guest bot's answer while the member is muted, or more than
  three joins of the same chat within 35 minutes turns it on; a member who stays quiet
  starts at the normal setting. A chaos failure deliberately keeps the ban, so the member
  stays in the chat's *Removed users* list and cannot rejoin.
- **Someone who is already in the chat is not affected.** Gating happens on a join, so a
  member who joined while the bot was misconfigured stays unmuted until they rejoin.
- **Making the Worker authoritative on timing** (rejecting suspiciously fast clicks) is
  not implemented: the DO records `elapsedMs` per pick but does not act on it yet.
- Local `wrangler dev` cannot receive Telegram updates — point a webhook at a tunnel, or
  POST a captured update at it yourself (see `.dev.vars.example`).

## The page

`/telegram?session=<uuid>` is the game with a session attached. It reads the session on
load — and again every 30 seconds — and then:

- shows the time left and the tries remaining in the top-left corner, counted on the
  **Worker's** clock: a snapshot carries `serverNow`, which the page anchors against its own
  monotonic clock, so a device whose clock is wrong still counts down correctly, and a tab
  that was in the background catches up by itself;
- plays the shuffle the Worker handed out (`rounds` and `special`) rather than its own, so
  the picks it scores are the ones the player actually saw;
- reports every pick, including the ones the player never made — running out of time posts
  slot 0, because it is the Worker that scores the attempt;
- names the outcome in the Worker's terms: *Verified*, *N tries left*, *Out of tries*,
  *Time is up*, with *Try Again* only while the session still has tries.

`/` (no session) remains the plain simulator: all of the above is skipped there, and that is
also what the desktop build does — `src/net.rs` gives the non-wasm target a transport that
does nothing, so `cargo run` stays a harness for the UI.
