# Hyperframes Composition Brief: KE8YGW Logger

## Objective
Create a short launch-style brag video for KE8YGW Logger, a local-first,
plugin-based amateur radio operations platform.

## Output
- Composition directory: `brag-output/composition/`
- Rendered video: `brag-output/brag.mp4`
- Format: landscape — 1920x1080, 30fps
- Duration: 23.46 seconds

## Source Material
- Project root: `/home/user/KE8YGW-logger`
- Primary files read: `crates/ham-client/web/index.html`,
  `crates/ham-client/web/styles.css`, `crates/ham-core/src/gui/shell.rs`,
  `docs/SECURITY_MODEL.md`, `docs/PLUGIN_SDK.md`, `README.md`, `Cargo.toml`
- Product name: KE8YGW Logger
- Tagline / strongest claim: plugins propose; the core decides what becomes
  official history. The log is append-only and local-first.
- Key UI moment to recreate: the Operating Deck shell — amber-on-near-black
  menubar, nine workspace mode buttons, omni search button, status bar — then
  the Callsign Entry panel accepting `K1ABC` and the resulting Recent QSOs row.
- Copy that must appear verbatim (all of it is real product text):
  - `KE8YGW` / `Local station` / `K8`
  - `Sync: Local only`
  - `Discovery: stopped / 0 peers`
  - `Runtime events: 0` → `Runtime events: 1`
  - `Errors: 0`
  - `KE8YGW Logger` (eyebrow) / `Dashboard` (workspace title)
  - `Search or run a command` + `Ctrl K`
  - `Dashboard`, `Casual Logger`, `POTA/SOTA`, `Maps`, `Awards`,
    `Online Services`, `Net Control`, `EmComm`, `Contesting`
  - `Callsign Entry`, `Recent QSOs`
  - `plugin_has_required_permission`
  - `operator_role_allows_permission`
  - `scope_allows_target_account_logbook_or_station`
  - `Ham Radio Operations Platform`
  - `K1ABC`, `N0BAD`, `W8ABC` and the QSO shape `20m / SSB / 59/59` — the
    repo's own test fixtures (`crates/ham-core/src/tests.rs`,
    `crates/ham-core/src/adif.rs`), not invented log data

## Creative Direction
- Tone preset: `polished`
- Creative direction: an operator's console at 2am — quiet, precise, amber on black
- Interpretation: restraint is the creative choice. One idea per scene, long
  settled holds, soft crossfades (0.6–0.8s), medium-weight mixed-case type with
  generous tracking. Energy comes from precision and from the amber, never from
  speed. Do not wink; this product is not a joke.
- Angle: the product is an operating console organized around one rule that
  lives in the repo as three literal lines of code. Plugins never write official
  history — they propose, and the core decides. The video makes that rule
  visible: type a callsign, watch the three checks pass, watch the QSO become
  official. Every string on screen is verbatim from the source.
- Hook: near-black; the amber `K8` mark strikes on, `KE8YGW / Local station`
  settles beside it, and `Sync: Local only` fades up at the bottom edge.
- Outro / punchline: `KE8YGW Logger` / `Ham Radio Operations Platform`, then
  `Local-first · Append-only · Desktop · iOS · Web · CLI`, bell into silence.
- Avoid:
  - Generic SaaS language
  - Abstract filler visuals
  - Unrelated visual redesign — the palette and type are the product's own
  - Any invented feature, metric, or claim not present in the source

## Visual Identity
From `crates/ham-client/web/styles.css`, `:root[data-theme="dark"]`.
- Background: `#0b0e13` (`--bg`); deck `#11151c`; panel `#161b23`; well `#0a0e14`
- Text: `#e8edf5` (`--text`); muted `#8b98ab`; faint `#5c6878`
- Accent: `#ffb545` (`--accent`); strong `#ffd08a`; ink `#221503`;
  wash `rgba(255,181,69,0.14)`; edge `rgba(255,181,69,0.45)`
- Support: ok `#3ddc97`; info `#4ecdff`; danger `#ff8f8f`
- Lines: `#262e3a` (`--line`); `#333d4d` (`--line-strong`)
- Display font: Inter (600/700, tracked out). Body font: Inter (400/500).
  Mono face for the three authorization identifiers — they are code.
- Visual references from the project: the `K8` brand mark tile; the menubar mode
  buttons with an amber-wash active state; the `⌘` omni button with its `Ctrl K`
  kbd; the panel cards on `--line` borders with `--shadow`; the dense status bar.

## Storyboard
Use the storyboard in `brag-output/brag-plan.md` as the creative contract.

Scene summary:
1. Power on — 3.27s (0.00→3.27) — `K8` + `KE8YGW / Local station`; `Sync: Local only`
2. The Operating Deck — 5.47s (3.27→8.74) — shell chrome; nine workspaces in three groups; status bar
3. A plugin proposes — 5.46s (8.74→14.20) — `K1ABC` types into Callsign Entry; three checks stamp in
4. The core decides — 5.46s (14.20→19.66) — `The core decides what becomes official.`; QSO row lands; `Runtime events: 1`
5. Lockup — 3.80s (19.66→23.46) — product lockup; `Local-first · Append-only · Desktop · iOS · Web · CLI`

## Audio
- Audio role: sparse professional accents over a low, warm bed
- Audio arc: bed fades in under the power-on, stays well below the accents
  throughout, and fades out from 21.8s so the closing bell rings into silence
- Music: `happy-beats-business-moves-vol-12-by-ende-dot-app.mp3` (109.96 BPM —
  the slowest bundled track, chosen to suit `polished`)
- Music treatment: start 0:00, bed volume ~0.32, fade in 0→0.8s, fade out
  21.8→23.46s. The bed is never the subject.
- Music cue guidance: bundled preset at
  `assets/music/cues/happy-beats-business-moves-vol-12-by-ende-dot-app.music-cues.json`.
  Three strong-cue locks (the guidance ceiling): **8.74s** (deck → proposal cut),
  **17.47s** (QSO row lands), **19.66s** (lockup). Beat-grid windows: workspace
  groups 4.39 / 6.00 / 7.09; authorization checks 10.37 / 11.46 / 12.55 (every
  other beat — 1.09s apart, above the reading floor).
- Audio-reactive treatment: subtle. Drive the `K8` mark's amber bloom and the
  Recent QSOs panel's presence from music RMS/bass. No waveform or equalizer
  graphics, no strobing, no text scaling.
- Audio-coupled moments:
  - Scene 1, 0.56s — mark landing — one warm soft impact
  - Scene 2, 4.39 / 6.00 / 7.09s — three workspace groups — one low-HF click each
  - Scene 3, 9.30→10.18s — typing `K1ABC` — varied per-character keypresses
  - Scene 3, 10.37 / 11.46 / 12.55s — each check stamping — one dry toggle each
  - Scene 4, 17.47s — QSO row landing — one warm impact (counter tick stays silent)
  - Scene 5, 19.66s — lockup — one deep bell, allowed to ring past the music
- SFX selection guidance: sound only where something visibly moves in that exact
  frame; nothing during the settled holds. `polished` takes fewer and quieter
  cues than any other preset.
- SFX analysis guidance: `skills/brag/assets/sfx/sfx-analysis.md`. Use
  low-high-frequency-risk files only — the `impact/impactSoft_medium_*` family
  and `interface/click_002|003|005` are the documented safest picks;
  `impact/impactBell_heavy_000` is the documented logo-payoff pick.
- Exact SFX choice: choose filenames, timestamps, density, and volume based on
  the implemented animation.
- Audio files: copy the chosen music and every selected SFX into
  `brag-output/composition/assets/`.

## Hyperframes Instructions
Load the composition-building Hyperframes domain skills — `hyperframes-core`,
`hyperframes-animation`, `hyperframes-creative`, `hyperframes-keyframes`, and
`hyperframes-cli`. /brag is its own workflow: do not enter the `hyperframes`
entry-point intent interview and do not route into its generic promo /
launch-video workflow. Prefer native Hyperframes conventions over anything in
`/brag`.

Requirements:
- Show at least one real UI, copy, or visual element from the source project.
- Keep all text readable in the final render — short labels ~0.8s settled, full
  sentences ~0.3s/word with a ~1.2s floor.
- Keep the video within 15-25 seconds (target 23.46s).
- Include the planned music/SFX layer.
- Treat `/brag` audio notes as guidance, not a fixed cue sheet. Choose SFX after
  the visual animation exists.
- Treat music cue metadata as optional timing hints; ignore cues that hurt
  readability, scene pacing, or the product story.
- Use only the three named strong-cue locks; align smaller entrances to nearby
  beats within ~0.10s.
- Use local assets for audio and any runtime dependencies.
- Run `hyperframes check` before render — it is brag's single gate.
