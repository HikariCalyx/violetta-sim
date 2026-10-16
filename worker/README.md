# violetta

A Cloudflare Worker that gates the members of a Telegram group behind the mini-game in
this repository. (`violetta` is the Worker's name in `wrangler.jsonc`, which is also what
the Cloudflare dashboard lists and what an API token scoped to one Worker would use.)

A new member is muted, given 10 minutes in total, and can only talk after finishing the
game in the bot's PM. Running out of time, or failing all three attempts, removes them
from the chat — they may rejoin and try again.

## How it fits together

| Piece | Where | Why |
| --- | --- | --- |
| Webhook + API routes | `src/index.ts` | One origin for the game page and the API, so there is no CORS |
| Telegram calls, permissions | `src/telegram.ts` | All Bot API traffic; every failure carries no token |
| Update dispatch, `/start`, `/verify_here` | `src/gating.ts` | The only place that decides "this member must be gated" |
| `ChatRegistry` DO | `src/registry.ts` | Which chats opt in. Strongly consistent on purpose |
| `UserIndex` DO | `src/user-index.ts` | Which session is open for a user, so a UUID in the URL is enough |
| `VerificationSession` DO | `src/session.ts` | The state machine, the countdown alarm, the round scoring |
| `dist/` | `../dist` | The trunk-built game, served by the same Worker |

One verification covers **every** protected chat the member has joined: finishing it
unmutes them everywhere at once, while each chat keeps its own 10-minute deadline. The
attempt counter is shared — one game, three tries.

## Setup

1. Create the bot with @BotFather and copy its token.
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
6. Add the bot to the group as an admin with **Delete messages** and **Ban users**, then
   send `/verify_here on` in the group. The bot verifies its own rights and refuses if
   either is missing.

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
| `GET /api/session/<uuid>/state` | `status`, `deadline`, `attemptsLeft`, the shuffle, and `serverNow` |
| `POST /api/session/<uuid>/pick` | `{ roundIndex, slot, elapsedMs }` — the only write the browser makes. A `slot` of 0 means "ran out of time": it scores as a miss instead of leaving the attempt open |
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
