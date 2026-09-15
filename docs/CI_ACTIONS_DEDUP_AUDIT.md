# GitHub Actions Duplication Audit

Audit of `.github/workflows/` for duplicated configuration, and the refactor
that followed. Findings F1–F3 and the Dependabot scope change are **implemented**;
F4–F9 were deliberately left alone. See [Outcome](#7-outcome) for what moved
where.

Companion document: [CI_OPTIMIZATION.md](CI_OPTIMIZATION.md) describes why the
jobs are shaped the way they are. This document only addresses repetition.

## 1. Inventory

As audited: six workflows, 1037 lines total, and no composite actions
(`.github/actions/` was absent). Line counts in the table below are the
pre-refactor figures the findings reference.

| Workflow | Lines | Triggers | Jobs |
| --- | --- | --- | --- |
| [ci.yml](../.github/workflows/ci.yml) | 481 | `pull_request` (dev, main), `push` (dev, main), `workflow_dispatch` | `promotion-policy`, `changes`, `preflight`, `rust-quality`, `platform-validation` (matrix: windows, macos), `tauri-validation`, `container-validation`, `ci-result`, `internal-artifact`, `beta-artifact` |
| [release.yml](../.github/workflows/release.yml) | 232 | `push` tags `v*.*.*`, `workflow_dispatch` (input: `tag`) | `validate-production-tag`, `release` (matrix: 3 OS), `attest-release-artifacts`, `publish-release` |
| [security.yml](../.github/workflows/security.yml) | 117 | `pull_request` (dev, main), `push` (dev, main), `schedule` `37 4 * * 1`, `workflow_dispatch` | `rust-dependencies`, `semgrep`, `workflow-lint` |
| [ios.yml](../.github/workflows/ios.yml) | 96 | `pull_request` (dev, main), `push` (dev, main), `workflow_dispatch` | `ios` |
| [scorecard.yml](../.github/workflows/scorecard.yml) | 79 | `branch_protection_rule`, `schedule` `16 3 * * 6`, `push` (main) | `analysis` |
| [branch-promotion-policy.yml](../.github/workflows/branch-promotion-policy.yml) | 32 | `pull_request` (dev, main) | `promotion-policy` |

Every workflow declares top-level `permissions: {}` except
`branch-promotion-policy.yml`, which declares top-level `permissions: contents: read`.

## 2. Findings

Line counts are of the duplicated regions only, excluding surrounding context.

| ID | Location(s) | What is duplicated | Times | Suggested fix | Workflow lines saved |
| --- | --- | --- | --- | --- | --- |
| F1 | `ci.yml` 168–180, 223–242, 272–282, 306–325; `ios.yml` 33–40; `release.yml` 111–117; `security.yml` 39–42 | `dtolnay/rust-toolchain` + `Swatinem/rust-cache` pair; `toolchain: 1.96.0` hardcoded at all 7 sites | 7 (toolchain), 6 (cache) | Composite action `setup-rust` | ~31 |
| F2 | `ci.yml` 229–236 (`rust-quality`), 312–319 (`tauri-validation`) | Byte-identical `apt-get` install of `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf` | 2 | Composite action `linux-desktop-deps` | ~12 |
| F3 | `ci.yml` 411–445 (`internal-artifact`), 447–481 (`beta-artifact`) | 35-line jobs differing only in channel name, dist dir, note text, retention days, and the `if` ref | 2 | Composite action `build-manifest` | ~34 |
| F4 | `ci.yml` 42–55 (`promotion-policy`); `branch-promotion-policy.yml` 19–32 | Identical checkout + `scripts/check-promotion-policy.sh` invocation; **also executes twice on every PR into `main`** | 2 | **No change — see below** | 0 |
| F5 | `security.yml` 77–82, 100–105 | `docker run --rm -v … -w /src "$SEMGREP_IMAGE" semgrep scan --metrics=off --config .semgrep` prefix | 2 | **No change — see below** | ~2 |
| F6 | `release.yml` 190–195 + 217–222, and glob lists 199–203 + 228–231 | Identical `download-artifact` block; near-identical `*.tar.gz`/`*.zip`/`*.sha256` glob lists | 2 | **No change — see below** | ~6 |
| F7 | `ci.yml` ×9, `security.yml` ×2, `ios.yml` ×1 | Job-level `permissions: contents: read` | 12 | **No change — see below** | 0 |
| F8 | `ci.yml` 16–21, `security.yml` 18–22, `ios.yml` 16–20 | `concurrency` block; group prefix (`ci-`/`security-`/`ios-`) is redundant with `${{ github.workflow }}` already in the key | 3 | **No change — see below** | 0 |
| F9 | 17 sites across all six workflows | `actions/checkout@08eba0b…` | 17 | **No change — see below** | 0 |

### Findings recommended for no change, and why

**F4 — promotion policy runs twice.** This is the largest *behavioural*
duplication found: for a PR targeting `main`, `scripts/check-promotion-policy.sh`
runs in both `CI / Main promotion policy` and
`Branch promotion policy / promotion-policy`. The logic is, however, already
deduplicated — the script is the single source of truth, and its header comment
(`scripts/check-promotion-policy.sh` lines 10–12) documents the arrangement
deliberately so the two checks "cannot drift apart". Only ~14 lines of YAML are
repeated.

Both job names are load-bearing:
[docs/security/REPOSITORY_SECURITY_SETTINGS.md](security/REPOSITORY_SECURITY_SETTINGS.md)
line 17 requires the "Branch promotion policy" check, and
[docs/BRANCHING_AND_RELEASE_CHANNELS.md](BRANCHING_AND_RELEASE_CHANNELS.md)
line 114 requires the main promotion policy. Converting either to a
`workflow_call` reusable workflow would rename its rendered check
(`Branch promotion policy / promotion-policy` becomes
`Branch promotion policy / promotion-policy / <inner job>`), silently
invalidating the required-status-check configuration on `main` and `dev`
branch protection. Deleting `ci.yml`'s job would remove it from the
`ci-result` gate and rename nothing but would drop a documented required check.

Recommendation: **leave both jobs as they are.** Fourteen duplicated lines are
the correct price for two independently-required checks whose shared logic is
already factored into a script. If the redundant execution is nonetheless worth
removing, that is a branch-protection change first and a YAML change second, and
needs an explicit go-ahead plus a coordinated ruleset update.

**F5 — Semgrep.** The composite action needed to share five lines would itself
cost ~22 lines. The two invocations also cannot be merged into one `semgrep scan`
call without changing behaviour (the first scan emits SARIF at all severities;
the second fails the build only on `ERROR`). Not worth it.

**F6 — release artifact download.** Same arithmetic: ~6 workflow lines saved
against ~18 lines of new action. Marginal; skip unless a third consumer appears.

**F7 — job-level `permissions`.** Hoisting `contents: read` to the top level
would be a **permissions change, not a refactor**. `ci-result` currently
inherits `permissions: {}` and needs no token at all; hoisting would grant it
`contents: read`. The existing top-level `{}` plus narrow per-job grants is also
exactly what the OpenSSF Scorecard `Token-Permissions` check rewards, and
`scorecard.yml` is in this repo. Recommendation: **do not hoist.** Recorded here
so a future reader does not "fix" it.

**F8 — `concurrency`.** GitHub provides no supported mechanism for sharing a
`concurrency` block across workflow files, and the officially-supported
alternatives do not apply. Report only.

**F9 — `actions/checkout`.** Seven of the 17 sites pass different options
(`fetch-depth: 0`, `persist-credentials: false`, `ref:`). A composite wrapper
would be a 2-line call replacing a 2-line `uses:` — zero saving — and would move
the pinned SHA out of Dependabot's default scan path. Leave as is; Dependabot
already keeps the 17 pins in lockstep.

## 3. Refactor

Three changes, in preference order from the audit brief. All three are composite
actions (option 2); no shared *job* is used across workflow files, so no
reusable workflow (option 1) is warranted, and no near-duplicate job survives
that a `strategy.matrix` (option 3) would serve better — see the note on F3.

### Structure

```
.github/
├── actions/                          # added
│   ├── setup-rust/
│   │   └── action.yml                # F1: toolchain pin + optional cargo cache
│   ├── linux-desktop-deps/
│   │   └── action.yml                # F2: apt packages for Tauri/webkit builds
│   └── build-manifest/
│       └── action.yml                # F3: channel build manifest writer
├── dependabot.yml                    # modified: add .github/actions to scan scope
└── workflows/
    ├── branch-promotion-policy.yml   # unchanged
    ├── ci.yml                        # uses setup-rust ×4, linux-desktop-deps ×2, build-manifest ×2
    ├── ios.yml                       # uses setup-rust ×1
    ├── release.yml                   # unchanged — see below
    ├── scorecard.yml                 # unchanged
    └── security.yml                  # uses setup-rust ×1
```

### F1 — `setup-rust`

Inputs: `toolchain` (default `1.96.0`), `components` (default
`rustfmt, clippy`), `cache` (default `true`), `cache-shared-key` (default `''`),
`cache-save-if` (default `'true'`, matching `Swatinem/rust-cache`'s own default).

Call sites and the inputs each must pass to stay behaviour-identical:

| Site | `components` | `cache` | `cache-shared-key` | `cache-save-if` |
| --- | --- | --- | --- | --- |
| `ci.yml` `preflight` | default | default | `linux-debug` | trunk expression |
| `ci.yml` `rust-quality` | default | default | `linux-debug` | trunk expression |
| `ci.yml` `platform-validation` | default | default | `${{ runner.os }}-debug` | trunk expression |
| `ci.yml` `tauri-validation` | default | default | `linux-debug` | trunk expression |
| `ios.yml` `ios` | `rustfmt` | default | default | default |
| `security.yml` `rust-dependencies` | `''` | `false` | — | — |

`release.yml` is **excluded**. Its `release` job checks out the tagged commit
(`ref: ${{ needs.validate-production-tag.outputs.sha }}`), so a local composite
action would be resolved from the tag's tree. The documented `workflow_dispatch`
path — "Existing production tag to validate without publishing" — runs the
current workflow file against an old checkout, so every tag created before this
refactor would fail to find `.github/actions/setup-rust`. Three saved lines are
not worth breaking tag re-validation, so `release.yml` keeps its inline setup
and the only remaining copy of the `1.96.0` pin.

"Trunk expression" is
`${{ github.ref == 'refs/heads/dev' || github.ref == 'refs/heads/main' }}`.
It is still written at four `ci.yml` sites.

> **Correction.** This audit originally proposed hoisting that expression into
> `ci.yml`'s top-level `env:` as `CARGO_CACHE_SAVE`. Verification against
> `Swatinem/rust-cache` at its pinned SHA disproved that: `src/config.ts` lines
> 118 and 127 hash every environment variable whose name starts with `CARGO`
> (also `CC`, `CFLAGS`, `CXX`, `CMAKE`, `RUST`) into the cache key. A variable
> whose value is `false` on pull requests and `true` on trunk would put pull
> request runs on a different cache key from the trunk runs that save the
> cache, silently ending all cache reuse in CI. The expression stays at the
> four call sites. Any future workflow-level variable must avoid those six
> prefixes.

`preflight` keeps its step-level `if: needs.changes.outputs.rust == 'true'` on
the composite call, which preserves the conditional exactly.

Two behaviour details to verify in review:

- `components: ''` must be a no-op for `dtolnay/rust-toolchain`, matching the
  three sites that pass no `components` today.
- `shared-key: ''` must be a no-op for `Swatinem/rust-cache`, matching
  `ios.yml` and `release.yml`.

Both were confirmed against the pinned SHAs rather than assumed:

- `dtolnay/rust-toolchain`'s `flags` step builds its `--component` arguments by
  iterating the comma-separated list, so an empty value contributes no flag.
- `Swatinem/rust-cache` gates on `if (sharedKey)` in `src/config.ts` line 77,
  so `''` is falsy and selects the automatic job-based key.

The payoff is not line count. It is that `1.96.0` currently appears in seven
workflow sites plus [rust-toolchain.toml](../rust-toolchain.toml); after this it
appears in one workflow site plus `rust-toolchain.toml`. A follow-up could have
the composite read the channel from `rust-toolchain.toml` and drop the pin
entirely, but that is a behaviour change and is out of scope here.

### F2 — `linux-desktop-deps`

No inputs. Wraps the byte-identical `apt-get update && apt-get install -y` block
from `rust-quality` and `tauri-validation`. Adding a system dependency becomes a
one-file change instead of a two-site edit that is easy to half-apply.

### F3 — `build-manifest`

Inputs: `channel`, `directory`, `note`. Output: `artifact` (the computed
artifact name). `internal-artifact` and `beta-artifact` keep their own job
names, their own `if:` conditions, and their own `retention-days` (7 and 30) on
the `upload-artifact` step.

One deliberate mechanism change with an identical observable result: the jobs
currently write `ARTIFACT_NAME` to `$GITHUB_ENV` and read it back as
`${{ env.ARTIFACT_NAME }}`. The composite action returns it as a step output
instead, which is the idiomatic form and avoids leaking a variable into the rest
of the job. The uploaded artifact name is unchanged.

**A `strategy.matrix` was considered and rejected for F3.** The two jobs are
selected by mutually exclusive job-level `if:` conditions on `github.ref`. A
matrix would either run both entries with every step skipped in one of them —
adding a permanently-skipped phantom job to the checks UI — or require a
dynamically generated matrix, which is more machinery than the duplication
costs. A composite action preserves the existing check surface exactly.

### Required companion change: Dependabot scope

[.github/dependabot.yml](../.github/dependabot.yml) currently scans the
`github-actions` ecosystem at `directory: /`, which covers
`.github/workflows/` only. Pinned SHAs moved into `.github/actions/*/action.yml`
would **stop receiving Dependabot updates** — a supply-chain regression that
`scorecard.yml` would eventually flag. The `github-actions` entry must be
switched to `directories:` including `/.github/actions/*` in the same commit
that introduces the first composite action.

This is not optional and is easy to miss; it is the main hidden cost of choosing
composite actions here.

## 4. Measured line-count effect

Composite actions move lines rather than delete them. Measured, not estimated:

| File | Before | After | Delta |
| --- | --- | --- | --- |
| `.github/workflows/ci.yml` | 481 | 424 | −57 |
| `.github/workflows/ios.yml` | 96 | 92 | −4 |
| `.github/workflows/security.yml` | 117 | 118 | +1 |
| `.github/workflows/` (all six) | **1037** | **977** | **−60** |
| `.github/actions/` (new) | 0 | 118 | +118 |
| **Total Actions configuration** | **1037** | **1095** | **+58** |

`security.yml` grows by one line because its job installs no cache, so the call
site spends two lines saying so (`components: ""`, `cache: "false"`) where the
old block spent one on the toolchain version. It still drops a copy of the pin.

The duplication removed is what matters, and it is not a line count:

| Thing | Before | After |
| --- | --- | --- |
| Copies of the `1.96.0` toolchain pin in workflows | 7 | 1 (`release.yml` only) |
| Copies of the apt package list | 2 | 1 |
| Copies of the build-manifest schema | 2 | 1 |
| Pinned third-party action SHAs outside Dependabot's scan scope | 0 | 0 |

## 5. Verification

The repo's own `Workflow lint` job (`security.yml`, `raven-actions/actionlint`)
covers syntax. Behaviour equivalence needed more than that. What was checked
before pushing:

1. **actionlint 1.7.7** — clean across all six workflows after each commit.
2. **YAML parse** — every workflow, every `action.yml`, and `dependabot.yml`.
3. **Upstream input semantics** — `dtolnay/rust-toolchain` and
   `Swatinem/rust-cache` were fetched at their pinned SHAs and read, rather
   than assumed, to confirm `components: ""` and `shared-key: ""` are exact
   no-ops. See the note under F1. The same reading is what caught the
   `CARGO_*` cache-key hazard recorded above.
4. **Footprint diff** — a script parsed each workflow before and after and
   compared triggers, top-level and job-level `permissions`, `concurrency`,
   workflow `env`, job names, `if` conditions, `runs-on`, `needs`, `strategy`,
   `outputs`, and secret references. All six workflows: identical.
5. **Effective step sequence diff** — the same script expanded every local
   composite action call back into its underlying steps (resolving input
   defaults and the `inputs.cache` condition) and compared the flattened
   sequence per job. Eighteen of twenty jobs: identical. The two exceptions
   are `rust-quality` and `tauri-validation`, reported as `REORDERED` with the
   same multiset of steps — the intended cache-before-apt reordering.
6. **Manifest output equivalence** — the old inline script and the new action
   body were both executed locally against the same `GITHUB_SHA`,
   `GITHUB_RUN_NUMBER`, and `GITHUB_RUN_ID`. Both channels produced
   byte-identical `BUILD-MANIFEST.txt` files and the same artifact names
   (`internal-dev-0.5.1-…`, `beta-main-0.5.1-…`).

What local checking cannot cover, and what a real run must confirm:

- That the runners resolve `./.github/actions/*` as expected. Opening a pull
  request into `dev` exercises every new call site: the PR touches
  `.github/workflows/*`, so `ci.yml`'s change detector sets `all_expensive=true`
  (`ci.yml` lines 109–111) and every gated job runs.
- Actual cache hit rates. Compare the restored cache key in the `Cache Cargo`
  step logs against a pre-refactor run on the same base; `linux-debug` and
  `<os>-debug` must be unchanged.

`release.yml` needs no run-time validation because it was not modified.
## 6. Trigger, permission, and secret footprint

The refactor changes **no** trigger, `permissions:` block, secret, or
`concurrency` group in any workflow — verified mechanically, see §5 item 4. No
workflow file was deleted or renamed. No new third-party marketplace action was
introduced: all three composite actions wrap actions the repo already used, at
their existing pinned SHAs.

The two findings that *would* have changed that footprint (F4 promotion-policy
consolidation, F7 permissions hoisting) are recommended against above and are
not part of this proposal.

## 7. Outcome

Implemented across three commits, one logical change each:

| Commit | Change |
| --- | --- |
| `ci: extract composite action for Rust setup` | `.github/actions/setup-rust/` + six call sites in `ci.yml`, `ios.yml`, `security.yml`; Dependabot scope |
| `ci: extract composite action for Linux desktop dependencies` | `.github/actions/linux-desktop-deps/` + two call sites in `ci.yml` |
| `ci: extract composite action for channel build manifests` | `.github/actions/build-manifest/` + `internal-artifact` and `beta-artifact` |

What moved where:

- The Rust toolchain version and cache wiring moved from six job bodies into
  `.github/actions/setup-rust/action.yml`. Call sites now declare only what
  differs: components, cache key, and whether the cache may be saved.
- The Ubuntu desktop package list moved from two `ci.yml` jobs into
  `.github/actions/linux-desktop-deps/action.yml`.
- The build-manifest schema moved from two `ci.yml` jobs into
  `.github/actions/build-manifest/action.yml`, which now returns the artifact
  name as a step output instead of via `$GITHUB_ENV`.
- `.github/dependabot.yml` switched its `github-actions` ecosystem from
  `directory: /` to a `directories` list that also covers `/.github/actions/*`.

Deliberately not changed: F4 through F9, for the reasons in §2. `release.yml` is
untouched for the tag-checkout reason in §3.

### Follow-ups a maintainer may want

- **`release.yml`'s toolchain pin.** It is the last copy. Migrating it safely
  means either accepting that pre-refactor tags can no longer be re-validated by
  dispatch, or checking the action out separately from the workflow ref.
- **Sourcing the toolchain from `rust-toolchain.toml`.** `setup-rust` could read
  the channel from the file that already pins it, removing the version from the
  workflows entirely. That is a behaviour change, so it was left out of a
  refactor commit.
- **F4's redundant execution.** Still runs the promotion policy twice on every
  pull request into `main`. Removing it is a branch-protection change first.
