//! The verification session this page was opened for.
//!
//! A link the bot hands out is `…/telegram?session=<uuid>`, and the Worker behind it
//! holds the whole verification: which chats the member is muted in, when their ten
//! minutes are up, and which shuffle they have to follow. The page's job is small —
//! read that session, show the player how long they have left, tell the Worker which
//! position they picked, and stop when it says the verification is over.
//!
//! The Worker is the only authority that matters for scoring: it hands out each
//! round's shuffle ([`Plan`]), and it counts the attempts. What the page does with
//! the answer is presentation.
//!
//! Only the wasm target ever fetches. The native build has the same types and the
//! same game screen, with a transport that does nothing, so `cargo run` stays a
//! useful harness for the UI without a session.

use std::sync::{Arc, Mutex, MutexGuard};

/// How many places the stage has. Mirrors `layout::ANSWER_COUNT`; repeated here so
/// this module stays independent of the screens that use it.
const STAGE_SLOTS: usize = 4;

/// How often the page re-reads the session while it is open.
///
/// The countdown is kept locally between reads, so this only exists to notice that
/// the Worker has moved on without us — the deadline passing, or an administrator
/// cancelling the verification.
const RESYNC_SECONDS: f64 = 30.0;

/// What the Worker reports about a session.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub status: String,
    /// The Worker's clock when it answered, in milliseconds since the epoch.
    pub server_now: f64,
    /// The earliest moment this session kicks the member, milliseconds; `None` once
    /// nothing is left to kick.
    pub deadline: Option<f64>,
    pub attempts_left: u32,
    /// `rounds[round][character] = slot`, one-based slots, exactly as the Worker
    /// shuffled them.
    pub rounds: Vec<Vec<u8>>,
    /// Which character wears the make-up, by index.
    pub special: u8,
    pub round_index: u32,
    /// Whether the Worker caught the member posting before the mute landed (or through
    /// a guest bot afterwards), which makes the challenge start in chaos mode.
    #[serde(default)]
    pub chaos: bool,
    /// The link came from the PM userbot, so there is no group membership to be
    /// removed from and the result screen must not mention one.
    #[serde(default)]
    pub userbot: bool,
}

/// What the Worker answered after a pick.
///
/// Only the wasm transport ever sees one; the game reads the same facts out of the
/// snapshot, which is updated from it.
#[cfg(target_arch = "wasm32")]
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PickReport {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub correct: bool,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub attempts_left: u32,
    /// The next attempt's shuffle, sent when the pick ended an attempt but the
    /// member still has tries left.
    #[serde(default)]
    pub rounds: Option<Vec<Vec<u8>>>,
    #[serde(default)]
    pub special: Option<u8>,
}

/// The shuffle the Worker chose, in the game's own terms: slots are zero-based, and
/// `orders[round][character]` is where that character has to end up.
#[derive(Debug, Clone)]
pub struct Plan {
    pub orders: Vec<Vec<usize>>,
    pub special: usize,
    /// Play the game in chaos mode, because the Worker flagged this member.
    pub chaos: bool,
}

/// Everything the fetch tasks write back, behind one lock.
///
/// The tasks run on the browser's event loop and the game reads this once a frame,
/// so the whole exchange is a plain shared cell rather than a channel: there is
/// only ever one thing in flight, and the last answer wins.
#[derive(Default)]
pub struct Cell {
    /// Bumped whenever new data lands, so the game can tell a fresh answer from one
    /// it has already acted on.
    pub revision: u64,
    pub snapshot: Option<Snapshot>,
    pub error: Option<String>,
}

pub type SharedCell = Arc<Mutex<Cell>>;

/// Never panic on a poisoned lock: a panic inside a fetch task would leave the page
/// with no UI at all, which is worse than reading possibly-stale data.
fn lock(cell: &SharedCell) -> MutexGuard<'_, Cell> {
    cell.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A live verification, as the page sees it.
pub struct Verification {
    id: String,
    cell: SharedCell,
    /// The frame time the current snapshot arrived at, and the Worker's clock at
    /// that moment. Their difference is what turns "now" into the Worker's clock.
    anchor: f64,
    anchor_server_now: f64,
    /// The revision this session has already anchored on.
    revision: u64,
    /// When the last read was started, and whether one is still in flight.
    synced: f64,
    loading: bool,
    /// Whether the first snapshot has already been checked for an attempt this page was
    /// reloaded into the middle of.
    recovered: bool,
}

impl Verification {
    /// Starts a session and kicks off the first read.
    pub fn open(id: String) -> Self {
        let verification = Self {
            id,
            cell: SharedCell::default(),
            anchor: 0.0,
            anchor_server_now: 0.0,
            revision: 0,
            synced: 0.0,
            loading: true,
            recovered: false,
        };
        spawn_load(&verification.cell, &verification.id);
        verification
    }

    /// Folds in whatever the fetch tasks have produced, and starts another read when
    /// this one has gone stale.
    pub fn refresh(&mut self, now: f64) {
        let (revision, snapshot) = {
            let cell = lock(&self.cell);
            (cell.revision, cell.snapshot.clone())
        };

        if revision != self.revision {
            // A snapshot carries the Worker's own clock, so anchoring here keeps the
            // countdown right even when the device's clock is not.
            self.revision = revision;
            self.anchor = now;
            self.anchor_server_now = snapshot.as_ref().map_or(0.0, |s| s.server_now);
            self.loading = false;

            // A page reloaded in the middle of an attempt is out of step with the
            // Worker's round counter, and the rounds it already scored cannot be
            // replayed — the reveal that showed which character to follow is gone, and
            // every later pick would be refused as out of order. So the interrupted
            // attempt is closed as a miss, once, and the Worker hands out a fresh one.
            if let Some(snapshot) = &snapshot {
                if !self.recovered {
                    self.recovered = true;
                    if snapshot.status == "open" && snapshot.round_index > 0 {
                        spawn_pick(&self.cell, &self.id, snapshot.round_index, 0, 0.0);
                    }
                }
            }
        }

        if self.loading || now - self.synced < RESYNC_SECONDS {
            return;
        }
        self.synced = now;
        self.loading = true;
        spawn_load(&self.cell, &self.id);
    }

    pub fn snapshot(&self) -> Option<Snapshot> {
        lock(&self.cell).snapshot.clone()
    }

    /// The last transport error, for the notice the page shows when it cannot read
    /// its session at all.
    pub fn error(&self) -> Option<String> {
        lock(&self.cell).error.clone()
    }

    /// How long the member has left, in seconds, on the Worker's clock.
    ///
    /// `f64::INFINITY` while the session has no deadline (nothing left to kick).
    pub fn remaining(&self, now: f64) -> f64 {
        let Some(snapshot) = self.snapshot() else {
            return f64::INFINITY;
        };
        let Some(deadline) = snapshot.deadline else {
            return f64::INFINITY;
        };
        let server_now = self.anchor_server_now + (now - self.anchor) * 1000.0;
        (deadline - server_now) / 1000.0
    }

    /// The shuffle to play, if the Worker has handed one out.
    ///
    /// Returns `None` for anything malformed rather than trusting it: a broken plan
    /// would turn the stage into nonsense, and the game can always fall back to its
    /// own shuffle.
    pub fn plan(&self) -> Option<Plan> {
        let snapshot = self.snapshot()?;
        let special = usize::from(snapshot.special);
        if special >= STAGE_SLOTS || snapshot.rounds.is_empty() {
            return None;
        }

        let mut orders = Vec::with_capacity(snapshot.rounds.len());
        for round in &snapshot.rounds {
            if round.len() != STAGE_SLOTS {
                return None;
            }
            let mut slots = Vec::with_capacity(STAGE_SLOTS);
            for slot in round {
                let index = usize::from(*slot).checked_sub(1)?;
                if index >= STAGE_SLOTS {
                    return None;
                }
                slots.push(index);
            }
            orders.push(slots);
        }

        Some(Plan {
            orders,
            special,
            chaos: snapshot.chaos,
        })
    }

    /// Tells the Worker which position was picked. `slot` is one-based, or zero when
    /// the player ran out of time without answering.
    pub fn report_pick(&self, round: usize, slot: u8, elapsed_ms: f64) {
        spawn_pick(&self.cell, &self.id, round as u32, slot, elapsed_ms);
    }
}

/// The `?session=` the bot put in the link, if this page was opened with one.
///
/// A page without it is the plain simulator — the public demo — which is why every
/// caller has to cope with `None` rather than assuming a verification.
#[cfg(target_arch = "wasm32")]
pub fn session_id() -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    let query = search.strip_prefix('?')?;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "session" && !value.is_empty()).then(|| value.to_owned())
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub fn session_id() -> Option<String> {
    // The desktop build is a harness for the same UI, never a verification.
    None
}

#[cfg(target_arch = "wasm32")]
mod transport {
    use std::sync::Arc;

    use eframe::wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::{JsFuture, spawn_local};

    use super::{PickReport, SharedCell, Snapshot, lock};

    /// Reads the session, and lets the caller know through the cell either way.
    pub fn spawn_load(cell: &SharedCell, id: &str) {
        let cell = Arc::clone(cell);
        let id = id.to_owned();
        spawn_local(async move {
            let url = format!("/api/session/{id}/state");
            let outcome = read_state(&url).await;
            let mut cell = lock(&cell);
            match outcome {
                Ok(snapshot) => {
                    cell.snapshot = Some(snapshot);
                    cell.error = None;
                }
                Err(error) => cell.error = Some(error),
            }
            cell.revision += 1;
        });
    }

    /// Reports one pick. A refusal means the Worker has already ended the session, so
    /// the session is read again to pick up the reason.
    pub fn spawn_pick(cell: &SharedCell, id: &str, round: u32, slot: u8, elapsed_ms: f64) {
        let cell = Arc::clone(cell);
        let id = id.to_owned();
        spawn_local(async move {
            let url = format!("/api/session/{id}/pick");
            let body = format!(
                r#"{{"roundIndex":{round},"slot":{slot},"elapsedMs":{}}}"#,
                elapsed_ms.round()
            );

            let report = match post_json(&url, &body).await {
                Ok(report) if report.ok => report,
                refused => {
                    // 404, 409 and 410 all mean the Worker has already moved on — the
                    // session ended while this page was open — and anything else means
                    // it was unreachable. Either way the page needs the current state,
                    // not a guess at it.
                    let message = match refused {
                        Ok(_) => "this verification is no longer running".to_owned(),
                        Err(error) => error,
                    };
                    lock(&cell).error = Some(message);
                    spawn_load(&cell, &id);
                    return;
                }
            };

            let mut cell = lock(&cell);
            // Fold the report into the snapshot rather than keeping two copies of the
            // same facts: a pick that ended the attempt can carry the next attempt's
            // shuffle, and the game reads its plan from the snapshot.
            if let Some(snapshot) = cell.snapshot.as_mut() {
                snapshot.status = report.status.clone();
                snapshot.attempts_left = report.attempts_left;
                if !report.correct {
                    if let Some(rounds) = &report.rounds {
                        snapshot.rounds = rounds.clone();
                        snapshot.round_index = 0;
                    }
                    if let Some(special) = report.special {
                        snapshot.special = special;
                    }
                }
            }
            cell.revision += 1;
        });
    }

    async fn read_state(url: &str) -> Result<Snapshot, String> {
        let (status, body) = send(url, None).await?;
        if status != 200 {
            return Err(format!(
                "the Worker could not read this session (HTTP {status})"
            ));
        }
        serde_json::from_str(&body).map_err(|error| format!("unreadable session: {error}"))
    }

    async fn post_json(url: &str, body: &str) -> Result<PickReport, String> {
        let init = web_sys::RequestInit::new();
        init.set_method("POST");
        init.set_body(&JsValue::from_str(body));
        let headers = web_sys::Headers::new().map_err(|error| describe(&error))?;
        headers
            .set("content-type", "application/json")
            .map_err(|error| describe(&error))?;
        init.set_headers(&headers);

        let (status, text) = send(url, Some(&init)).await?;
        // A 404 or 410 still carries a report-shaped body (`{ok:false,…}`), and the
        // caller re-reads the session anyway.
        serde_json::from_str(&text).map_err(|_| format!("the pick was refused (HTTP {status})"))
    }

    /// Sends a request and returns its status and body.
    async fn send(url: &str, init: Option<&web_sys::RequestInit>) -> Result<(u16, String), String> {
        let window = web_sys::window().ok_or_else(|| "there is no window".to_owned())?;
        let promise = match init {
            Some(init) => window.fetch_with_str_and_init(url, init),
            None => window.fetch_with_str(url),
        };
        let value = JsFuture::from(promise).await.map_err(|error| describe(&error))?;
        let response: web_sys::Response = value
            .dyn_into()
            .map_err(|_| "the request did not answer with a response".to_owned())?;

        let status = response.status();
        let body = JsFuture::from(response.text().map_err(|error| describe(&error))?)
            .await
            .map_err(|error| describe(&error))?;
        Ok((status, body.as_string().unwrap_or_default()))
    }

    /// A failed fetch rejects with a `TypeError` rather than a string, so fall back
    /// to its debug form.
    fn describe(error: &JsValue) -> String {
        error
            .as_string()
            .unwrap_or_else(|| format!("{error:?}").replace("JsValue(", "").replace(')', ""))
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod transport {
    use super::SharedCell;

    pub fn spawn_load(_cell: &SharedCell, _id: &str) {}

    pub fn spawn_pick(_cell: &SharedCell, _id: &str, _round: u32, _slot: u8, _elapsed_ms: f64) {}
}

use transport::{spawn_load, spawn_pick};
