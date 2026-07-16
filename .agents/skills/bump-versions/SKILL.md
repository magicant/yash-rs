---
name: bump-versions
description: 'Apply the yash-rs versioning rules: bump affected crate versions, sync the root Cargo.toml, and add Unreleased changelog headings, per the PR template. Documentation updates are out of scope (see the update-docs skill).'
argument-hint: 'Which crates/behavior changed in this PR?'
---

# Bump Versions and Sync Release Metadata

This skill is the **canonical procedure** for the workspace's version- and
changelog-related rules. The
[PR template](../../../.github/pull_request_template.md) deliberately omits these
details so contributors are not burdened with them; the maintainer applies them
at review time using this skill.

Use this skill after the code for a PR is written, to make the repository's
release metadata consistent with the changes: crate version numbers, the root
`Cargo.toml` dependency versions, and each affected `CHANGELOG.md`.

**Scope.** This skill owns the version-number and changelog-heading rules and the
root `Cargo.toml` sync. Related concerns are delegated:

- For the changelog *wording and categorization* details, defer to the
  [update-changelog skill](../update-changelog/SKILL.md).
- **Documentation under `docs/src` is out of scope.** When a user-visible change
  also needs documentation, handle that separately with the
  [update-docs skill](../update-docs/SKILL.md). This skill does not edit docs.
- For staging and committing the resulting changes, hand off to the
  [commit skill](../commit/SKILL.md).

## When to Use

- A PR's code changes are complete and the version/changelog metadata must catch
  up.
- The user asks to bump versions, sync `Cargo.toml`, or add changelog entries
  for a PR.
- A behavior or public-API change needs version + changelog updates.

## Inputs to gather first

1. **The diff.** `git diff` (and the merge base) to see every changed file.
2. **Which crates are touched**, directly or transitively.
3. **The kind of change per crate** (patch / minor / major — see classification).

## Procedure

### Step 1 — Identify affected crates

From the diff, list every crate that needs a metadata update. A crate is
"affected" if any of the following holds:

- its own source/manifest changed, **or**
- it directly depends (in `[dependencies]`; also check `[dev-dependencies]` for
  completeness, though those don't affect published-crate compatibility) on
  another workspace crate whose version changed in this cycle — **public or
  private** dependency alike.

**Find these dependents explicitly; do not rely on the initiating diff to
surface them.** A version bump of `yash-env`, say, potentially affects *every*
crate with `yash-env = { workspace = true }`, not just the one crate whose
source you were asked to change. Run `grep -l '^<crate> = ' */Cargo.toml` for
each crate bumped in Step 3 and add every hit (that isn't the crate itself) to
the affected list — this is the step most easily skipped, because the code
diff usually only touches one dependent directly.

Decide later (Step 2 / Step 4) the version-bump *severity* for each affected
crate; "affected" only means it needs attention here. But do not assume a
dependency bump alone skips the version bump — see the note in Step 4's
cross-crate propagation about public vs. private only changing *severity* and
*root-requirement* handling, never whether a bump happens at all.

### Step 2 — Classify the change for each crate

First decide the **change category**, then map it to a version bump using the
crate's current major version.

- **Library crates (all except `yash-cli`):** classify by **public API**.
  - Breaking API change → *breaking*.
  - Backward-compatible API addition → *compatible*.
  - No API change (internal only, bug fix) → *patch-level*.
- **`yash-cli` (binary):** classify by **observable shell behavior**.
  - Behavior change or new feature → *compatible*; bug fix → *patch-level*.
  - `yash-cli` re-exports nothing, so only observable behavior drives its
    version. A dependency bump alone never bumps `yash-cli` (though it is still
    recorded in its changelog per Step 5).

**Version mapping.** Every crate in this workspace follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Because most crates
are still pre-1.0, apply **Cargo's 0.x convention** (as documented in each
crate's `CHANGELOG.md` preamble, clarified in commit `9d7a76ba`):

| Crate major version | Breaking change | Compatible change | Patch-level |
| ------------------- | --------------- | ----------------- | ----------- |
| `0.y.z` (pre-1.0)   | bump `y` → `0.(y+1).0` | bump `z` → `0.y.(z+1)` | bump `z` → `0.y.(z+1)` |
| `≥ 1.0.0`           | bump major      | bump minor        | bump patch  |

So while the major version is `0`, a breaking change bumps the **minor** `y`
(resetting `z` to 0), and any backward-compatible change — including a bug fix —
bumps the **patch** `z`. Check each crate's current version to pick the right
column; the workspace currently mixes `0.y.z` crates with a few `1.x` ones
(`yash-executor`, `yash-fnmatch`, `yash-quote`).

### Step 3 — Bump the crate version in each crate's `Cargo.toml`

For each affected crate that needs a version bump, set `version` in
`<crate>/Cargo.toml` to the forecast next release version.

The forecast version is **the latest published release bumped exactly once, by
the most severe change category accumulated across *all* unreleased work** — not
the current change alone, and not one bump per change.

Compute it like this:

1. Find the **latest published release** version — the newest `[x.y.z]` release
   heading in `<crate>/CHANGELOG.md` (ignore any `Unreleased` heading).
2. Determine the **highest-severity category among all unreleased changes** so
   far, including this PR's change. Severity order: *breaking* > *compatible* >
   *patch-level*.
3. Apply that single bump to the latest release using the Step 2 mapping.

The `[x.y.z] - Unreleased` heading is an **output** of this computation, not an
input: always (re)derive the forecast from latest-release + highest-accumulated
severity, then make the heading and `Cargo.toml` match it. Two consequences:

- **No double bump for the same severity.** If the Unreleased version already
  reflects a bump of the same or higher severity than this PR's change, the
  recomputation yields the same value — leave it as is. Example: latest release
  `1.2.3`, Unreleased already `1.3.0` (minor), and this PR adds another
  backward-compatible change → still `1.3.0`.
- **Raise the forecast when this PR is more severe.** If this PR outranks the
  bump the Unreleased version currently reflects, the recomputation yields a
  higher value — rewrite both the forecast and the `[x.y.z] - Unreleased`
  heading. Example: latest release `1.2.3`, Unreleased `1.3.0` (minor), and this
  PR adds a **breaking** change → rewrite to `2.0.0`. (In 0.x, the analogous
  case is `0.2.3` → Unreleased `0.2.4` (patch) + breaking change → `0.3.0`.)

### Step 4 — Sync the root `Cargo.toml`

The workspace dependency table in the root
[Cargo.toml](../../../Cargo.toml) (the `yash-* = { path = ..., version = ... }`
lines) holds, for each workspace crate, the **single workspace-wide minimum
version requirement** that all dependents inherit via `workspace = true`. This
table is the **driver**: when you raise a requirement here, every dependent's
required version rises at once, and Step 5 then records that bump in each
affected dependent's changelog mechanically. Decide the root here first; never
let a changelog entry drive the root value (that would be circular).

The `version` in the table is a Cargo **caret** requirement (`"0.2.3"` means
`>=0.2.3, <0.3.0`), and it is intentionally kept at the *lowest* version that
all dependents actually need — it is **not** automatically bumped just because
the crate released a new version. For each crate `X` bumped in Step 3, decide
whether to raise its root requirement using these three cases:

1. **Forced — X's bump is breaking** (in 0.x: a minor bump; in 1.x: a major
   bump). The old caret requirement excludes the new version, so dependents
   cannot build against it. **Always raise** the root requirement to X's new
   version. Mechanical; no confirmation needed.

2. **Internal-only compatible bump — leave the requirement unchanged.** If X's
   Unreleased changelog entries consist *only* of changes that expose nothing
   new to dependents — namely:
   - a Rust / MSRV version bump,
   - internal fixes that do not touch the public API, and/or
   - private-dependency version bumps —

   then no dependent can depend on anything new in X, so **keep the root
   requirement as is**. Mechanical; no confirmation needed. (A Rust/MSRV bump
   stays in this case **regardless of whether the changelog files it under
   "Public" or "Private dependency versions"** — what matters is that it adds no
   adoptable API. Example: `yash-arith 0.2.4`, whose only Unreleased change is a
   Rust version bump listed under *Public dependency versions*, still leaves the
   root requirement correctly at `0.2.3`.)

3. **Public-API compatible bump — ask the maintainer (default: leave).** If X's
   Unreleased changelog includes any public-API addition or change, a dependent
   *could* now adopt it, but this skill cannot reliably tell from the diff
   whether one actually did. **Ask the maintainer** whether any dependent now
   requires the new X and the workspace requirement should be raised. Default to
   leaving the requirement unchanged if unconfirmed.

When you do raise a requirement (cases 1 and 3-confirmed), set the root table
entry to X's new version.

**Cross-crate propagation:** for every crate that depends on X (found in Step
1), recording X's new version is itself a changelog-worthy change and — per
the Step 2 version-mapping table, where *compatible* and *patch-level* both
bump `z` by exactly the same amount in 0.x — **normally earns the dependent
its own patch-level (`z`) version bump, regardless of whether the dependency is
public or private.** This holds even when X's bump was itself only
patch-level, and even when nothing else about the dependent changed (real
precedent: `yash-fnmatch` 1.1.1 → 1.1.2 solely for a *private* `thiserror`
patch bump; `yash-syntax`, `yash-semantics`, and `yash-prompt` have each
released solely for a *public* `yash-env` bump). Public vs. private changes
two things only:

- **Which changelog list the entry goes in** — "Public dependency versions" vs
  "Private dependency versions" (Step 5; see "How to tell public from private"
  below).
- **Whether a *breaking* upstream bump propagates as breaking.** If the
  dependency is **public** (its items are re-exported) and X's bump was
  breaking (case 1, i.e. a minor bump in 0.x), the propagated severity is
  breaking too — loop back to Step 2 for that crate instead of defaulting to
  patch-level. If the dependency is **private**, the propagated severity stays
  capped at patch-level even when X's own bump was breaking, since nothing in
  the dependent's own public API changed.

Whether the *root* requirement for X itself gets raised is the separate
question decided just above (cases 1–3) — do not conflate "does the root
requirement rise" with "does this dependent get a changelog entry and version
bump." The two are independent: a crate can (and often does) get a patch bump
purely to record a dependency note even while the root requirement for that
dependency stays put (case 2).

**How to tell public from private — trust the changelog history.**
Do **not** try to re-derive a dependency's public/private status from the source
each time. Instead:

1. Look in the dependent crate's `CHANGELOG.md` for the most recent prior
   version that classified the dependency under a "Public dependency versions"
   or "Private dependency versions" list (or a line like "X is now a private
   dependency"). Assume the dependency keeps that classification now.
   - Example: `yash-prompt`'s `[0.13.0]` lists `yash-env` and `yash-syntax`
     under **Public dependency versions**, so treat both as *public* for the
     next `yash-prompt` release (e.g. `0.13.1` / `0.14.0`) unless the changelog
     later reclassifies them.
2. **If no prior entry records the classification** (a newly added dependency,
   or one never mentioned), do **not** guess — **stop and ask the maintainer**
   whether it is public or private before continuing.
3. **Do not proactively audit** existing classifications for correctness. But if
   you *incidentally* notice a contradiction or error (e.g. the changelog calls
   a dependency private while the crate clearly re-exports its types), **report
   it to the maintainer** rather than silently fixing or relying on it.

### Step 5 — Update changelogs

For each affected crate, ensure `<crate>/CHANGELOG.md` has an `[x.y.z] -
Unreleased` heading for the forecast version and an entry describing the change.
Follow the [update-changelog skill](../update-changelog/SKILL.md) for category
choice, wording, net-state representation, and the release-link reference.

Rules specific to this workflow:

- **`yash-cli` always gets an Unreleased heading when shell behavior changes**,
  even if `yash-cli` itself was not otherwise modified.
- **Dependency changes are mentioned.** If a `Cargo.toml` dependency was added,
  removed, or updated, note it in the changelog. List **private and public**
  dependency changes separately.

> **Documentation is out of scope here.** If the change is user-visible, the
> `docs/src` pages (and the "introduced in `yash-cli` x.y.z" version mention)
> still need updating — do that separately with the
> [update-docs skill](../update-docs/SKILL.md). This skill stops at versions and
> changelogs.

### Step 6 — Validate

- Each crate that got a version bump has a matching `[x.y.z] - Unreleased`
  changelog heading at the same version.
- The root `Cargo.toml` entry for a crate matches that crate's new version
  **only when Step 4 raised its requirement** (cases 1 and 3-confirmed).
  Otherwise the root deliberately stays at the lower caret minimum — do **not**
  "fix" that gap (e.g. `yash-arith` crate `0.2.4` with root requirement `0.2.3`
  is correct under Step 4 case 2).
- `Cargo.lock` is regenerated (run `cargo build` / `cargo test`) so it is
  consistent with the manifests; include it with the version bumps when the
  changes are committed (via the [commit skill](../commit/SKILL.md)).
- `./check.sh` passes.
- **Always run `./check-semver.sh`.** It runs `cargo semver-checks` over every
  library crate and confirms each crate's version bump is consistent with its
  actual public-API changes — the authoritative backstop for the Step 2/3
  classification. Run it whenever this skill changed any version, which is every
  invocation; do not treat it as optional.

  Know its limits, though — passing it does **not** prove the versions are fully
  correct:
  - It checks **library crates' public API surface only**.
  - It does **not** cover `yash-cli` (a binary); its version, driven by
    observable shell behavior, still rests on the Step 2 judgment.
  - It cannot catch a behavioral change that leaves the API unchanged, so the
    judgment behind a compatible (patch-level) bump is not validated here.

## Limits

- Do not bump a version that was already bumped to a sufficient severity for the
  current unreleased cycle (Step 3).
- Do not invent changelog entries for changes not present in the diff.
- Do not edit `docs/src`; defer documentation to the update-docs skill.
