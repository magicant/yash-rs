---
name: commit-message
description: 'Draft informative commit messages from repository changes. Use when writing a git commit message, summarizing staged edits, or turning a diff into a clear subject and body.'
argument-hint: 'What changed, or which diff should be summarized?'
---

# Informative Commit Messages

Use this skill when the goal is to produce a commit message that explains both what changed and why it matters.

## When to Use

- The user asks for a commit message or help polishing one
- There is a staged or unstaged diff that needs to be summarized
- A change spans multiple files and needs one coherent commit narrative
- The current draft is too vague, too long, or missing the reason for the change

## Outcome

Produce:

- A one-line subject that states the main change precisely
- A subject written as a plain imperative phrase, not a marker prefix
- An optional body that captures motivation, important details, and user-visible effects
- Wording that matches the actual diff instead of inventing intent

## Procedure

1. Inspect the intended commit scope.
   Decide whether to describe staged changes, unstaged changes, or a specific set of files. If the scope is unclear, ask or state the assumption.

2. Identify the core change.
   Reduce the diff to one primary action such as fixing a bug, adding behavior, refactoring an internal path, updating docs, or adjusting tests.

3. Separate what changed from why.
   Extract the concrete edits first, then identify the reason, regression, invariant, or user-facing effect. If the reason is not visible in the diff, say so instead of guessing.

4. Choose the message shape.
   Use a subject only for small self-explanatory changes. Add a body when the change affects behavior, touches multiple areas, has follow-up constraints, or needs context for reviewers.

5. Write the subject line.
   Prefer the imperative mood. Keep it specific and compact. Target 50 characters or fewer when possible, and keep the subject at 72 characters or fewer.
   Do not start the subject with a marker prefix such as `fix:`, `feat:`, or `fix(scope):`. Unless the user explicitly requests that convention, write a plain sentence-style subject (for example, `Preserve redirection order in subshell execution`).

6. Write the body if needed.
   Explain the motivation, notable implementation detail, and observable impact in short paragraphs or flat bullets. Wrap every body line at 72 characters. Mention tests only when they add meaning.

7. Validate against the diff.
   Check that every claim is supported by the changes. Remove filler words, avoid generic verbs like "add", "fix", "update" or "improve", and make sure unrelated edits are not bundled into the description.
   Confirm the subject does not match marker patterns like `^[a-z]+(\([^\)]*\))?!?:`.

## Decision Points

- If one change clearly dominates, center the subject on that change.
- If the diff contains unrelated edits, recommend splitting commits before drafting a final message.
- If the change is internal-only, emphasize the code path or invariant rather than inventing user impact.
- If the change is user-visible, name the behavior change explicitly.
- If the diff is mostly mechanical, describe the transformation and skip invented motivation.

## Quality Checks

- The subject is understandable without opening the diff.
- The subject names the real change, not a vague activity.
- The subject does not begin with marker prefixes such as `type:` or `type(scope):`.
- The body explains why the change exists when that is not obvious.
- The message does not claim behavior or intent that the diff does not support.
- The wording is short enough to scan quickly but detailed enough to be useful in history.

## Output Format

Unless the user asks for a different convention, always present the primary suggested commit message in a fenced `text` code block so it can be pasted directly into a commit editor.

Use this structure:

```text
<subject>

<optional body>
```

Keep the subject at 72 characters or fewer, preferably 50 or fewer, and wrap each body line at 72 characters.

If useful, also provide 2 to 3 alternative subject lines with different emphasis, such as user-visible behavior, subsystem, or bug fix framing. The alternatives can be outside the code block.

## Heuristics

- Prefer "Avoid parser panic on empty alias expansion" over "Improve alias handling"
- Prefer "Preserve redirection order in subshell execution" over "Refactor subshell code"
- Prefer "Document startup file precedence" over "Update docs"

## Limits

- Do not invent rationale that is not present in the diff or user context
- Do not hide mixed-purpose changes behind an overly broad message
- Do not optimize for style guides that the user did not request
