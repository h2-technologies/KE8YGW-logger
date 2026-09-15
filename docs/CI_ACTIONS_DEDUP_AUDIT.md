# GitHub Actions Duplication Audit

Audit of `.github/workflows/` for duplicated configuration, with a proposed
conservative refactor. **No workflow file was modified for this audit.** Every
change below is a proposal pending maintainer approval.

Companion document: [CI_OPTIMIZATION.md](CI_OPTIMIZATION.md) describes why the
jobs are shaped the way they are. This document only addresses repetition.

## 1. Inventory

Six workflows, 1037 lines total. No composite actions exist yet
(`.github/actions/` is absent).

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

## 3. Proposed refactor

Three changes, in preference order from the audit brief. All three are composite
actions (option 2); no shared *job* is used across workflow files, so no
reusable workflow (option 1) is warranted, and no near-duplicate job survives
that a `strategy.matrix` (option 3) would serve better — see the note on F3.

### Resulting structure

```
.github/
├── actions/                          # new
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
    ├── release.yml                   # uses setup-rust ×1
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
| `release.yml` `release` | `''` | default | default | default |
| `security.yml` `rust-dependencies` | `''` | `false` | — | — |

"Trunk expression" is
`${{ github.ref == 'refs/heads/dev' || github.ref == 'refs/heads/main' }}`.
It is still written at four `ci.yml` sites. To single-source it, hoist it into
`ci.yml`'s existing top-level `env:` as e.g. `CARGO_CACHE_SAVE` and pass
`cache-save-if: ${{ env.CARGO_CACHE_SAVE }}` — workflow-level `env` may use the
`github` context, so this is safe.

`preflight` keeps its step-level `if: needs.changes.outputs.rust == 'true'` on
the composite call, which preserves the conditional exactly.

Two behaviour details to verify in review:

- `components: ''` must be a no-op for `dtolnay/rust-toolchain`, matching the
  three sites that pass no `components` today.
- `shared-key: ''` must be a no-op for `Swatinem/rust-cache`, matching
  `ios.yml` and `release.yml`.

Both actions read these via `core.getInput`, which returns `''` for an omitted
input, so passing `''` explicitly is equivalent — but this is the one assumption
in the whole refactor that a real run should confirm.

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

## 4. Expected line-count effect

Composite actions move lines rather than delete them. Honest accounting:

| | Workflow lines | Action lines | Net |
| --- | --- | --- | --- |
| F1 `setup-rust` | −31 | +40 | +9 |
| F2 `linux-desktop-deps` | −12 | +13 | +1 |
| F3 `build-manifest` | −34 | +42 | +8 |
| **Total** | **−77** | **+95** | **+18** |

`.github/workflows/` drops from 1037 to ~960 lines; the Actions configuration as
a whole grows by ~18 lines. The duplication removed is what matters: the Rust
toolchain pin goes from 7 copies to 1, the apt package list from 2 to 1, and the
build-manifest schema from 2 to 1.

If a net line *increase* is not an acceptable outcome, F2 alone is close to
break-even and F1 is the only change that removes a real drift hazard; F3 could
be deferred.

## 5. Verification plan

The repo's own `Workflow lint` job (`security.yml`, `raven-actions/actionlint`)
covers syntax. Behaviour equivalence needs more than that:

1. Land each change as its own commit, per the brief.
2. Open a PR into `dev`. This fires `CI`, `Security scanning`, and `iOS Native`,
   which between them exercise every `setup-rust` call site except
   `release.yml`'s.
3. Because the PR touches `.github/workflows/*`, `ci.yml`'s change detector sets
   `all_expensive=true` (ci.yml lines 109–111), so every gated job runs — the
   refactor gets full coverage automatically.
4. Compare the per-step logs against a pre-refactor run on the same base:
   resolved `rustc --version`, the restored cache key, and the apt package set
   must match.
5. `release.yml`'s `setup-rust` site is **not** covered by a PR run. Validate it
   via `workflow_dispatch` with the `tag` input against an existing production
   tag — that path runs `validate-production-tag` and `release` but skips
   `attest-release-artifacts` and `publish-release` (both gated on
   `github.event_name == 'push'`), so it builds and packages without publishing.
6. Confirm the uploaded artifact names from `internal-artifact` on a `dev` push
   match the pre-refactor naming exactly.

Nothing is removed until the replacement has run green.

## 6. Trigger, permission, and secret footprint

The proposed refactor changes **no** trigger, `permissions:` block, secret, or
`concurrency` group in any workflow. No workflow file is deleted or renamed. No
new third-party marketplace action is introduced — all three composite actions
wrap actions the repo already uses at their existing pinned SHAs.

The two findings that *would* have changed that footprint (F4 promotion-policy
consolidation, F7 permissions hoisting) are recommended against above and are
not part of this proposal.

## 7. Status

Audit complete; **no files under `.github/` have been modified.** Implementation
of F1–F3 and the Dependabot scope change awaits maintainer approval.
