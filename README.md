# violetta_lie_detector_simulator

A WebAssembly (Rust) re-implementation of the *Lie Detector* mini-game.

## Requirements

- Rust (edition 2024, so 1.85+; developed against 1.97)
- The wasm target: `rustup target add wasm32-unknown-unknown`
- [Trunk](https://trunkrs.dev): `cargo install --locked trunk`

## Layout

```
index.html                 Trunk entry point (builds the crate below, and the favicon)
Trunk.toml                 Trunk build settings: the wasm-opt version it uses
scripts/                   Tools for the build, the second one run in CI
    check-wasm-table.mjs   Guards the built wasm against a breaking wasm-opt
    subset-cjk-font.py     Builds the fonts the localised names are drawn with
Resources/                 Sprite artwork, baked into the binary at build time
    bg.jpg                 The title screen's background photograph
    2190000.png            The item's icon, which is also the page's favicon
    sprite-pivots.txt      Image origins, one sprite per line
    <lang>/                Artwork translated into that language (cn, jp, kr, tw)
Reference/                 Gameplay recordings used as a reference (git-ignored)
violetta_lie_detector_sim/ The Rust crate
    build.rs               Indexes Resources/ into the sprite and animation tables
    fonts/                 Text fonts: one subset per language
    src/main.rs            Native + wasm entry points
    src/app.rs             App state and the screen switch
    src/assets.rs          Sprite lookup and the texture cache
    src/net.rs             The verification session, and the shuffle the Worker scores
    src/screens/start.rs   Title screen
    src/screens/result.rs  Success / Failed screen
    src/screens/game/      The lie detector window
        layout.rs          Pixel geometry, measured from the artwork and the footage
        stage.rs           Maps the window artwork onto the screen
        characters.rs      The four characters, their poses and their swaps
        pattern.rs         The faint figures the stage lamps throw up
        rng.rs             A small random generator, seeded per game
        widgets.rs         The answer buttons
```

### Sprites

Every PNG in `Resources/` — and the one JPEG, the title screen's photograph — is
embedded by `build.rs`, so artwork is available by name with no code change:
`lieDetector.back1` is a sprite, and `lieDetector.npc.stand.3` is frame 3 of the
`lieDetector.npc.stand` animation. The animation groups (including their frame
order, and any gaps in the frame numbering) are discovered from the file names too.

The sprites are pixel art and are sampled nearest-neighbour; a photograph is not,
so `Textures::get_smooth` decodes one with linear filtering for artwork that is
resized to fit rather than drawn at its own size.

### Languages

A subdirectory of `Resources/` named after a language holds the artwork translated
into it, and stands in for the base sprite of the same name while that language is
shown: `Resources/kr/lieDetector.back1.png` is the Korean window frame, and the
three `textMessage` sprites and the standby dialog are translated with it. What a
language does not carry is drawn from the base artwork — which is the English one —
so a language only has to translate what it actually has, and English needs no
directory at all.

Which language is shown is the platform's answer, not the player's: the browser's
`navigator.language` in the wasm build, or `LANGUAGE`/`LC_ALL`/`LANG` natively (see
`src/language.rs`). A tag is matched on its primary subtag, with the region or
script deciding between the two Chinese translations (`zh-TW` and `zh-Hant` get
`tw`, anything else Chinese gets `cn`), and a language nothing is translated into
falls back to the base artwork.

Each language also gives the app itself a name — `language::title` — which is the
character's name as that language's artwork writes it (`碧欧蕾塔` in Simplified
Chinese, `菲歐娜` in Traditional Chinese, where the localisation renamed her, `ビオルタ`
in Japanese and `비올레타` in Korean) followed by that language's own word for the
device. It is the heading of the title screen, and it is what goes in the browser
tab or the window title.

The screens the app composes itself are translated too — `language::words` — since
an English button under a translated window would be the one thing left over: the
*Start* button, the *Try Again* button, the *Success* / *Failed* headings the result
screen reports the outcome with, and the chrome the bot's verification adds around
them — the page that says it is reading the session, the retry line under the
verification clock, and *Verified* / *Time is up* / *Out of tries* with the line that
follows them. One string there is a template rather than a sentence: `tries_left`
carries `$count` for the number, and `language::Words::tries_left` fills it in —
English, the only one of these languages that inflects, keeps a separate wording for
a single attempt beside it. The photograph's credit line is left in English, because
the video it credits is an English title, as are the reasons `net.rs` reports when a
session cannot be read: they name what went wrong rather than talking to the player.

What is still English in every language is the text baked into the artwork: the
title bar inside the window frame, the three instruction lines, and the success and
fail banners. Those are sprites, not strings, so translating them means
redrawing them.

### The name fonts

egui's bundled fonts — Ubuntu and Hack — have no CJK in them at all, so a localised
string would be drawn as replacement boxes. Each language therefore has a font of
its own: `violetta_lie_detector_sim/fonts/<language>.otf` is a subset of the Noto
Sans CJK the platform ships, holding only the glyphs that language's strings use — a
few KB rather than the 9-17 MB of the whole font, and each language keeps its own
glyph forms. All four together are ~65 KB. `scripts/subset-cjk-font.py` builds them
from the strings `language.rs` spells out, and `cargo test` fails if one of them is
missing a glyph, so changing a name, a button label or an outcome tells you to run it
again:

```sh
py scripts/subset-cjk-font.py
```

The fonts are licensed under the SIL Open Font License, which is next to them.

### Pivots

Each animation frame is trimmed to its own drawing, so the images differ in size
and in where the drawing sits inside them. Drawn corner-to-corner they would
shake, so `Resources/sprite-pivots.txt` records the *origin* every image was
authored with:

```
< sprite name > < x > < y >
```

A sprite is drawn so that this point lands on the slot it was placed at, which
is what holds a frame still. A sprite that has no line in the file is drawn from
its corner, which is only right for full-window artwork.

The file is transcribed from the game's own UI data (the `origin` vector of each
image), which is not part of this repository; adding artwork means adding a line.

## Run in the browser (wasm)

```sh
trunk serve --open
```

Then edit any file and reload: `trunk serve` rebuilds on change.
For a deployable bundle in `dist/`, run `trunk build --release` — a release build
is post-processed with wasm-opt, whose version `Trunk.toml` pins.
`node scripts/check-wasm-table.mjs dist/*_bg.wasm` then re-runs the bundle's
startup in Node and fails if that wasm-opt left it unable to start; the release
build in CI does the same.

## Run natively (faster builds, handy for iterating)

```sh
cargo run --manifest-path violetta_lie_detector_sim/Cargo.toml
```

The same UI code drives both targets, so a native run is a quick way to check
layout and behaviour before building the wasm bundle.

## Screens

- **Start** – `Resources/bg.jpg` behind it, scaled to cover the screen and cropped
to it, with the title, a *Start* button, and the photograph's credit in the
bottom-left corner: “Background Image Credit: ” and the name *Somacguffin*, which
links to [the video the still came from](https://www.youtube.com/watch?v=K7MTgHeFrYc).
- **Game** – the lie detector window.
- **Verification** – not a screen of its own: opened as `…/telegram?session=<uuid>` the app
reads that session from the Worker, counts its clock down in the top-left corner, shuffles
the characters the way the Worker scored them, and reports each pick. A *time left* of zero
ends the page where it stands.

### How the window is composed

`lieDetector.back1.png` is the window frame: a title bar, the instruction strip,
the detector device, and a black 412x187 opening. The frame is drawn *last*, so
it acts as a mask for the layers under it:

1. `lieDetector.back.png`, the theatre stage, at the top-left of the opening;
2. the animated layer — characters, the glow and the stamps;
3. the pattern the lamps throw up — two dozen faint copies of `npc.stand.0` at
   random places and strengths, laid over the characters and scattered afresh
   every round;
4. the stage lights, at 60% and cycling colour twice a second once the masks are
   on — over the characters, so the light falls on them, and still under the
   frame;
5. the window frame;
6. the instruction line, the device's number readout, and the answer buttons.

All coordinates are in window space (the frame artwork is 441x318 px) and live in
`src/screens/game/layout.rs`; `stage.rs` scales that onto the screen, snapping to
a whole-number scale when the window is enlarged.

### A round

One of the four characters is the special one — it is the only one whose eyes are
open, the others look asleep. A round runs through `Phase` in
`src/screens/game/mod.rs`:

| Phase | What happens | How long |
| --- | --- | --- |
| `Countdown` | the game's standby dialog, with the seconds left written into the blank its instruction line leaves before "sec until the Lie Detector mini-game starts." | 1 s per digit, 5 digits |
| `Reveal` | the four characters appear in the front row, one to a slot, and the glow rests on the special one | 5 s |
| `Ready` | the `READY` stamp | the 12 frames of the stamp |
| `Start` | the `START` banner over the window | the 5 frames of the banner |
| `MaskOn` | the masks go on while the stage lights come up to 60%, flicking colour twice a second from then on | the 6 frames of the mask |
| `Swap` | the characters walk to new slots, each at a depth of its own, all looking alike | 2.5 s, the game's own shuffle animation delay |
| `Pick` | the answer buttons are live; the pick lands in the readout, and the device's gauge drains as its number counts the seconds down | 5 s, or until a click |
| `MaskOff` | the lights go out, then the masks come off | fade + the 6 mask frames |
| `Success`/`Failed` | the matching banner, then the result screen | the frames of the banner |

Each character stands in a *slot* (left to right) and a *row* (front to back), and
they all stand in the front row (`layout::FRONT_ROW`), one to a slot — lined up
across the stage and never overlapping, which is what the game does for the reveal
and again for the picks.

A round sends each of them back to a row of its own: `layout::WALK_ROWS` hands out
every row behind the front one, one each, so the four are never at the same depth
while they shuffle. Since the masks make the four identical, that depth is all that
tells them apart when their paths cross: a row further back is drawn smaller
(`layout::DEPTH_SCALE`, a fifth a row — the deepest is two fifths the size of the
front one) and a little higher up the window (`layout::DEPTH_STEP`).

A round does not send them straight to their new slot either: each one is given a
path of four places — where it stands, two places at its own depth, and where it
ends up — so it steps upstage (shrinking as it goes), wanders across behind the
others, and comes forward again to full size. Every character on the move plays the
walk cycle, including one that was handed its own slot back and so walks out and
back to where it started; drawing one of them from the stand instead would leave it
sliding across the stage as a still picture. The walk cycle (`npcCommon.move`) shows
the character from behind, walking away, so one cycle serves every direction of
travel, and the four are drawn furthest-back first, so a character in front covers
one behind it.

An answer can be given either way: the numbered button under a character, or the
character itself, which is a click target while the round is waiting for one — and
whichever character the pointer is over is framed with a thin rectangle, so it is
clear what a click would choose.

`Swap` and `Pick` repeat four times, once per readout cell: each round that is
picked adds the position that was clicked to the device's readout, in `number1`
only. A round that runs out of time ends the game right there — the rounds that
were left are never played — and goes straight to `MaskOff`, so it always ends in
*Failed*. Every attempt having been right is a *Success*; either way, the result
screen offers *Try Again*, which starts over at the countdown.

The characters are drawn in `src/screens/game/characters.rs`. Positions and swaps
are random (`rng.rs`, a xorshift64* so that the game needs no dependency).

### The instruction line

The line in the strip along the top of the frame is the game's own message
artwork, centred at `layout::BANNER_CENTRE` — not text this app composes, which
is what lets it come out in the original lettering. The three messages are
numbered in the order the game uses them, and each is shown for the phases it
describes:

| Phase | Message |
| --- | --- |
| `Reveal`, `Ready`, `Start` | `lieDetector.textMessage.0` — *Keep your eyes on the Violetta who's wearing make-up.* |
| `MaskOn`, `Swap`, `Pick` | `lieDetector.textMessage.2` — *Click the button to select the Violetta wearing make-up!* |
| `MaskOff`, `Success`, `Failed` | `lieDetector.textMessage.1` — *We'll give the correct answer, and if you answered incorrectly you will be busted for using macros.* |

The countdown is drawn on `lieDetectorStandBy.back`, the game's own dialog for
waiting on a game to start, and its number uses that dialog's digit set
(`lieDetectorStandBy.number`), placed at `layout::STANDBY_COUNT` — the blank the
dialog leaves in its instruction line.

## Credits
- Background Image: [somacguffin](https://youtube.com/@somacguffin)
- Other image assets are property of Nexon.