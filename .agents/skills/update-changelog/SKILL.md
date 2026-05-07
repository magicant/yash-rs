---
name: update-changelog
description: 'Update crate CHANGELOG.md files in yash-rs. Use for adding unreleased entries, deciding changelog categories, and keeping changelogs aligned with crate and workspace version changes.'
argument-hint: 'Which crate(s) changed and what behavior changed?'
---

# Update Changelog Entries

Use this skill to update changelog entries for one or more crates in this workspace.

## When to Use

- The user asks to update or add a `CHANGELOG.md` entry
- A code change is complete and needs release notes
- A crate version changed and changelog updates are needed
- A behavioral change in the shell should be documented for users

## Outcome

Produce changelog updates that:

- Use an `[x.y.z] - Unreleased` section for each affected crate
- Add or update the release-link reference for each newly added unreleased section
- Use standard categories: Added, Changed, Deprecated, Removed, Fixed, Security
- Describe the user or developer impact clearly and concretely
- Stay consistent with crate versions in `Cargo.toml`
- Order bullets by importance and relatedness, not insertion time

## Procedure

1. Identify affected crates.
   Use the diff to find which crates changed. Treat each crate independently.

2. Confirm target version for each crate.
   Read `<crate>/Cargo.toml` and capture the current forecast version.
   Do not bump versions again if already bumped for unreleased work.
   If the version does not look like the next semver release for the current changes,
   stop and ask the user how to resolve the mismatch before editing changelogs.

3. Open `<crate>/CHANGELOG.md`.
   Find an existing `[x.y.z] - Unreleased` section matching the crate version.
   If missing, create it near the top using repository conventions.
   When creating a new unreleased section, also add a link definition near the end of
   the changelog that points to the corresponding release page.

4. Classify each change.
   Map each change to exactly one primary category:
   - Added: New feature or capability
   - Changed: Behavior change that is not purely a bug fix
   - Deprecated: Still available but planned for removal
   - Removed: Deleted behavior or API
   - Fixed: Bug correction
   - Security: Vulnerability or hardening update

5. Write concise, user-meaningful bullets.
   State what changed and why it matters. Avoid internal-only jargon unless the crate is developer-facing.
   For `yash-cli/CHANGELOG.md`, keep entries focused on user-visible behavior and
   avoid implementation details aimed at crate developers.
   Order bullets by importance first, then by relatedness within a category.

6. Handle cross-crate behavior.
   If shell-observable behavior changed, also update `yash-cli/CHANGELOG.md` even when implementation lives in another crate.

7. Keep scope clean.
   Only add entries for changes in the current work. Do not rewrite historical sections.

8. Validate consistency.
   Ensure each edited changelog version matches the corresponding crate version and wording is factual.

## Decision Points

- If change is not user-visible but affects crate users (library API or semantics):
  add an entry to that crate changelog.

- If change is observable in shell behavior:
  add or update entry in `yash-cli/CHANGELOG.md`.

- If `Cargo.toml` version does not appear semver-appropriate for the pending changes:
   ask the user whether to adjust the version, scope, or changelog framing.

- If a change could fit multiple categories:
  choose the category users will search first, then keep the bullet focused.

- If no relevant changelog section exists for the target unreleased version:
  create `[x.y.z] - Unreleased` before adding bullets.

## Quality Checks

- Every affected crate has matching changelog coverage.
- Category headings are consistent with repository style.
- Bullets describe behavior or impact, not vague implementation activity.
- In `yash-cli/CHANGELOG.md`, bullets stay user-facing and avoid deep internal details.
- Bullets are ordered by importance and relatedness, not by edit time.
- No duplicate bullets across categories.
- `yash-cli` changelog is updated when user-visible shell behavior changed.
- No contradiction between changelog text and code changes.

## Output Format

When reporting back to the user:

- List edited changelog files
- Summarize bullets added per file
- Mention any crates checked but intentionally not updated
- Call out any follow-up needed (for example, unclear release framing)

## Limits

- Do not invent behavior changes that are not present in code or tests.
- Do not add speculative future work to changelog entries.
- Do not mix unrelated changes into one bullet.
