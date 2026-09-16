# Brag Plan: KE8YGW Logger

## What is this app?
A local-first, plugin-based amateur radio operations platform — casual logging,
POTA/SOTA, nets, contesting, and EmComm on one append-only official log, built on
a shared Rust core that ships to desktop, native iOS, hosted web, and a CLI.

## The angle
Most logging software is a database with a form on top. This one is an *operating
console* with a constitution. The whole product is organized around one rule that
lives in `docs/SECURITY_MODEL.md` as three literal lines of code:

```
plugin_has_required_permission
AND operator_role_allows_permission
AND scope_allows_target_account_logbook_or_station
```

Plugins never write official history — they propose, and the core decides. The
video's job is to make that rule *visible*: type a callsign, watch the three
checks pass, watch the QSO become official. No abstraction, no metaphor — the
product's actual amber-on-black shell doing its actual thing.

Specificity comes free here: every string on screen is copied verbatim out of
`crates/ham-client/web/index.html`, `crates/ham-core/src/gui/shell.rs`, and
`docs/SECURITY_MODEL.md`. Nothing is invented marketing language.

## Hook (first 2-3 seconds)
Near-black. The amber `K8` brand mark strikes on with a soft bloom, `KE8YGW /
Local station` settles beside it, and one real status-bar string fades up at the
bottom: **`Sync: Local only`**.

That is the thesis in three words. This thing is complete with the internet
unplugged — and it is the app's literal default state, not a claim about it.

## Key moments (the middle)
- The Operating Deck assembling: the nine real workspaces (Dashboard, Casual
  Logger, POTA/SOTA, Maps, Awards, Online Services, Net Control, EmComm,
  Contesting) arriving in three groups, with the real `⌘ Search or run a command
  · Ctrl K` omni button and the real status bar.
- A callsign typed into the real **Callsign Entry** panel: `K1ABC` — the repo's
  own fixture callsign (`crates/ham-core/src/tests.rs`) — character by character,
  with keypress ticks.
- The three authorization checks stamping in one at a time, in the project's own
  identifiers — the single most specific thing this product owns.
- The QSO landing at the top of **Recent QSOs** and the status bar's
  `Runtime events` counter ticking `0 → 1`. The proposal became history.

## Outro / punchline
Back to near-black. `KE8YGW Logger` / `Ham Radio Operations Platform`, and a
quiet footer that earns the whole video: `Local-first · Append-only · Desktop ·
iOS · Web · CLI`. Bell rings over the fading bed. Silence.

## User flow worth showing
Entry → key action → result, exactly as the code does it:
1. **Entry** — the Callsign Entry panel takes `K1ABC`.
2. **Key action** — the plugin submits a proposal; the core runs
   `plugin_has_required_permission AND operator_role_allows_permission AND
   scope_allows_target_account_logbook_or_station`.
3. **Result** — an official event is appended, `Recent QSOs` gains the row, and
   the runtime event counter moves. Scenes 3 and 4 are the centerpiece; the
   landing-shell scene is the frame around them, not a substitute.

## Tone
- Preset: `polished`
- Creative direction: an operator's console at 2am — quiet, precise, amber on black
- Interpretation: restraint is the point. Slow crossfades, one idea per scene,
  long settled holds, no bullets, medium-weight mixed-case type with generous
  tracking. The product is not a joke and the video must not wink. Energy comes
  from precision and from the amber, not from speed.

## Format: landscape — 1920x1080
## Duration: 23.5 seconds

## Visual identity (from the project)
Taken from `crates/ham-client/web/styles.css`, dark theme (`:root[data-theme="dark"]`).
- Background: `#0b0e13` (`--bg`); deck `#11151c`, panel `#161b23`, well `#0a0e14`
- Accent: `#ffb545` (`--accent`), strong `#ffd08a`, wash `rgba(255,181,69,0.14)`,
  edge `rgba(255,181,69,0.45)`
- Text: `#e8edf5` (`--text`); muted `#8b98ab`; faint `#5c6878`
- Support: ok `#3ddc97`, info `#4ecdff`, danger `#ff8f8f`
- Lines: `#262e3a` (`--line`), `#333d4d` (`--line-strong`)
- Display font: Inter (the project's only family — weight and tracking carry the
  display/body distinction rather than a second face)
- Body font: Inter; code/identifiers in a mono face (the three checks are code)
- Strongest visual element: amber on near-black at low saturation — a radio
  console lit for night operation. The `K8` brand mark is the one bold shape.

## Share copy (draft)
Introducing KE8YGW Logger: a local-first ham radio operations platform where
plugins propose and the core decides what becomes official.

## Audio direction
- Role: sparse professional accents over a low, warm bed
- Music: `happy-beats-business-moves-vol-12-by-ende-dot-app.mp3` — the slowest
  bundled track (109.96 BPM), which suits `polished` better than the faster vols
- Music treatment: start at 0:00, bed volume ~0.32 (well under the SFX), fade in
  0 → 0.8s, fade out 21.8 → 23.46s so the final bell rings into silence
- Music cue guidance: preset read from
  `assets/music/cues/happy-beats-business-moves-vol-12-by-ende-dot-app.music-cues.json`
  (tempo 109.96, 42 beats and 12 strong cues inside the 0–23s window).
  Strong-cue locks (3, the guidance ceiling): **8.74s** deck → proposal cut,
  **17.47s** QSO row lands, **19.66s** lockup. Beat-grid windows: workspace
  groups at 4.39 / 6.00 / 7.09; the three checks at 10.37 / 11.46 / 12.55
  (every *other* beat — 1.09s apart, comfortably over the reading floor for a
  code identifier). Both windows were pulled earlier than first drafted so the
  last item in each still holds ≥0.8s settled before its scene cuts.
- Audio-reactive treatment: subtle; use music RMS/bass so the `K8` mark's amber
  bloom and the Recent QSOs panel's presence breathe. No waveform or equalizer
  visuals, no strobing, no text scaling.
- SFX posture: sparse, motion-matched, low high-frequency risk throughout —
  `polished` gets fewer and quieter cues than any other preset
- Audio-coupled moments: the mark landing; three workspace groups arriving;
  per-character typing of `K1ABC`; each authorization check stamping; the QSO row
  landing; the final lockup bell
- Delivery master: the composition's authored balance is the creative contract,
  but the first render came out at -25.2 LUFS — far too quiet to share. All
  gains were raised +7 dB uniformly at source (bed 0.32 -> 0.72, every SFX
  slot x2.24), which preserves the balance exactly, and the delivery pass adds
  +6 dB into an oversampled limiter at -1.5 dBFS. Final: -13.7 LUFS,
  -1.2 dBTP, LRA 4.8 — on the social target with the dynamics untouched.
- Restraint rule: no sound fires unless something visibly moves at that exact
  frame. Nothing bright, hissy, or clicky (`sfx-analysis.md` low-HF-risk picks
  only). No sound at all during the settled holds — the holds are the tone.

## Storyboard

### Scene 1 — Power on — 3.27s (0.00 → 3.27)
Near-black `#0b0e13` field, slight vignette. The `K8` brand mark strikes in at
centre-left in `--accent` amber with a soft radial bloom; `KE8YGW` (bold) and
`Local station` (muted) settle to its right. At the lower edge, one real status
string fades up in `--faint`: `Sync: Local only`. Everything is settled by ~1.6s
and simply holds — the hold is the hook.
Sequential/interaction: none — one mark, one lockup, one status line.
Audio intent: a console coming up. One warm landing, then room tone.
Audio-coupled idea: the mark's landing at 0.56 takes a single soft impact.
Music: low warm bed, faded in across 0 → 0.8s.
Transition mood: soft → Scene 2

### Scene 2 — The Operating Deck — 5.47s (3.27 → 8.74)
Pull back into the real shell chrome. The menubar assembles: brand block at
left, `⌘ Search or run a command  Ctrl K` omni button at right, `theme` and
`Workspace` selects implied. Title row reads eyebrow `KE8YGW Logger` over `H1
Dashboard`. The nine workspace mode buttons arrive as **three groups of three**
— (Dashboard, Casual Logger, POTA/SOTA) / (Maps, Awards, Online Services) /
(Net Control, EmComm, Contesting) — then the full row holds from 7.51 → 8.74.
Along the bottom, the real status bar settles: `Sync: Local only` ·
`Discovery: stopped / 0 peers` · `Runtime events: 0` · `Errors: 0`.
Sequential/interaction: yes — three grouped reveals on the beat grid at 4.39 /
6.00 / 7.09. Grouped rather than one-by-one on purpose: nine labels on nine
consecutive 0.55s beats would outrun reading, and it would feel busier than
`polished` permits.
Audio intent: quiet competence — the deck laying itself out.
Audio-coupled idea: one low-HF selection click per group, not per button.
Transition mood: clean cut on the strong cue → Scene 3

### Scene 3 — A plugin proposes — 5.46s (8.74 → 14.20)
Push into the **Callsign Entry** panel, panel-dark `#161b23` on a `--line` edge.
A small amber eyebrow above it reads `A plugin proposes.` A caret types `K1ABC`
into the field between 9.30 and 10.18, one character at a time. Then the three
authorization checks stamp in below, in mono, one per reveal, each gaining a
`--ok` `#3ddc97` check as it lands:
`plugin_has_required_permission` (10.37) ·
`operator_role_allows_permission` (11.46) ·
`scope_allows_target_account_logbook_or_station` (12.55).
All three hold together to the cut. Text verbatim from `docs/SECURITY_MODEL.md`.
Sequential/interaction: yes — simulated typing, then three stamped reveals on
every other beat (1.09s apart, above the reading floor for single identifiers).
Audio intent: deliberate. Each check is a decision being made, not a notification.
Audio-coupled idea: randomized keypress ticks per character; one soft, dry toggle
per check landing exactly on its reveal frame.
Transition mood: clean → Scene 4

### Scene 4 — The core decides — 5.46s (14.20 → 19.66)
The checks recede and the **Recent QSOs** panel comes forward. A line settles at
~15.8s in `--text` with amber emphasis: `The core decides what becomes official.`
At 17.47 a new row slides into the top of the list and flashes `--accent-wash`
once: `K1ABC · 20m · SSB · 59/59` — the exact QSO fixture the repo's own tests
use. The two rows already in the log (`N0BAD`, `W8ABC`) slide down one slot. In the same frame the status bar ticks
`Runtime events: 0` → `Runtime events: 1`, `Errors: 0` unchanged and steady.
Sequential/interaction: yes — the row arrival and the counter tick are one
simultaneous beat, deliberately not staggered. The proposal became history in a
single instant; splitting them would weaken it.
Audio intent: the payoff, but understated — weight, not celebration.
Audio-coupled idea: one warm low-HF impact on the row landing; the counter tick
is silent so the impact reads as a single event.
Transition mood: soft crossfade → Scene 5

### Scene 5 — Lockup — 3.80s (19.66 → 23.46)
Back to near-black. The `K8` mark and `KE8YGW Logger` land at 19.66 with
`Ham Radio Operations Platform` in `--muted` beneath. At ~21.3s a quiet footer
row fades up in `--faint`: `Local-first · Append-only · Desktop · iOS · Web · CLI`.
Music fades out from 21.8 so the bell is the last thing heard.
Sequential/interaction: none — lockup, then footer.
Audio intent: a full stop. Resonance, then nothing.
Audio-coupled idea: one deep bell on the lockup, allowed to ring past the music.
Transition mood: hold to black

**Music mood for this video:** warm, restrained, professional — a low bed, never the subject
**Audio summary:** a console powers up under a quiet bed, the deck lays itself out
in three soft clicks, a callsign types and three checks stamp, one warm impact
marks the QSO becoming official, and a single bell rings the lockup into silence.
