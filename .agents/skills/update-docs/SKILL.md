---
name: update-docs
description: 'Update the mdBook user documentation under docs/src in yash-rs. Use when a behavior or feature change needs corresponding documentation edits, or when adding, revising, or correcting existing documentation pages.'
argument-hint: 'Which page(s) or topic needs updating, and what changed?'
---

# Update Documentation Pages

Use this skill to update the user-facing documentation in [docs/src](../../../docs/src/),
which is built into an mdBook site hosted at <https://magicant.github.io/yash-rs/>.

## When to Use

- A code change alters shell-observable behavior that the docs describe
- The user asks to add, revise, or correct a documentation page
- A new feature or built-in needs to be documented
- Existing documentation is outdated, inaccurate, or incomplete

## Overview

The documentation is a set of Markdown files under `docs/src`, organized as an mdBook.
[docs/src/SUMMARY.md](../../../docs/src/SUMMARY.md) is the table of contents that defines the
page hierarchy and must stay in sync with the actual files. Pages are written in prose for
shell users (not crate developers), favor concrete runnable examples, and cross-link related
topics with relative Markdown links. Code examples are verified by `docs/doctest.sh`, and the
whole site is built and checked by `check-docs.sh`, so edits must keep examples correct and
the book buildable.

## Conventions

Follow these project-specific conventions when writing or editing pages. Match the style of
the surrounding pages — read a nearby page first if unsure.

### Linking

Follow the linking rules in [docs/README.md](../../../docs/README.md):

- Use relative Markdown links to other documentation files.
- To link to a `README.md` page from a file **other than `SUMMARY.md`**, point at the
  rendered `index.html` (e.g. `../interactive/index.html`), not `README.md`. This works
  around <https://github.com/rust-lang/mdBook/issues/984>.
- In `SUMMARY.md` only, refer to files by their true pathnames (including `README.md`).

A consequence: Markdown lint tools may flag the `index.html` links as broken because the
source file is named `README.md`. That is expected. What matters is that **mdBook emits
correct links in the final HTML** — do not "fix" these links to satisfy the linter.

Reference-style links are common in these pages (e.g. `[variable]` with a
`[variable]: ../language/parameters/variables.md` definition at the bottom of the file). Reuse
existing definitions where a page already has them.

### Version tags for new features

When the shell gains a new feature or option, begin the relevant paragraph with a
`(Since x.y.z)` tag indicating the version that introduced it, where `x.y.z` is the
`yash-cli` release version. Existing examples:

- [docs/src/environment/traps.md](../../../docs/src/environment/traps.md) — `(Since 3.1.0) In an interactive shell …`
- [docs/src/environment/options.md](../../../docs/src/environment/options.md) — `(Since 3.0.0) If set, the shell returns …`

Common mistakes to avoid:

- **Wrong position**: do NOT embed the tag in the middle or end of a sentence. It must be
  the very first thing on the first line of the paragraph.
- **Wrong format**: do NOT write `(since yash-rs x.y.z)`. The correct form is `(Since x.y.z)`:
  capital S, no "yash-rs" prefix.

### Naming and indexing keywords

- When a feature has a distinctive name, introduce it in **bold** the first time it appears
  (e.g. ``The **`cd`** built-in changes …``). Subsequent mentions are not bolded.
- Add such keywords to the index in
  [docs/src/topic_index.md](../../../docs/src/topic_index.md) so they are discoverable, and
  keep the entries alphabetically sorted (`docs/indexsort.sh`, run by `check-docs.sh`, checks
  this). Index-worthy names include **built-in utilities** and **shell options**, but **not**
  the individual options supported by a built-in utility.

### Code blocks: `shell` vs `sh`

Choose the fence language deliberately, because `docs/doctest.sh` treats them differently:

- ` ```shell ` — a shell **session**. Lines starting with `$ ` and `> ` are commands; the
  remaining lines are their expected output. These blocks are **executed and their output
  verified by default**, so make sure the example actually produces the shown output.
- ` ```sh ` — a shell **script**. The whole block is parsed and syntax-checked, but not
  executed.

For `shell` blocks that should not be run as-is, attach attributes after the language
(comma-separated):

- `no_run` — syntax-check only, do not execute (e.g. commands with side effects or that block).
- `hidelines=<prefix>` — strip the given prefix from each line before processing, to hide
  setup lines from the rendered page (e.g. ` ```shell,hidelines=# `).
- `one_shot` — check the combined output once instead of per command/output pair.
- `ignore` — skip the block entirely.

## Workflow

1. Identify the affected page(s) under `docs/src`. For a new page, add it to `SUMMARY.md`.
2. Edit the Markdown, applying the conventions above (linking, `(Since x.y.z)` tags, bold
   keyword introductions, `topic_index.md` entries, and the right code-block language/flags).
3. Run `./check-docs.sh` from the repository root and confirm it finishes with **no errors
   and no warnings**. This builds the book with mdBook, runs `docs/doctest.sh` on the
   examples, and verifies the index sort order.
4. Fix any reported failures and re-run until clean before committing.
