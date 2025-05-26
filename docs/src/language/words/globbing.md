# Pathname expansion (globbing)

**Pathname expansion**—also known as **globbing**—lets you use special patterns to match filenames and directories. The shell expands these patterns into a list of matching pathnames.

For example, `*.txt` matches all files in the current directory ending with `.txt`:

```shell,no_run
$ echo *.txt
notes.txt todo.txt
```

## When does pathname expansion happen?

Pathname expansion occurs after [field splitting](field_splitting.md) and before quote removal. It only applies to unquoted words containing globbing characters.

If the `noglob` shell option is set, pathname expansion is skipped.

## Pattern syntax

Pathname expansion uses [shell patterns](../../patterns.md). Patterns may include special characters such as `*`, `?`, and bracket expressions. See [Pattern matching](../../patterns.md) for a full description of pattern syntax and matching rules.

The following subsections describe aspects of pattern matching specific to pathname expansion.

### Unmatched brackets

Unmatched brackets like `[a` and `b]c` are treated as literals. However, some shells may treat other glob characters as literals if they are used with unmatched open brackets. To avoid this, make sure to quote unmatched open brackets:

```shell,no_run
$ echo [a
[a
$ echo \[*
[a [b [c
```

### Subdirectories

Globs do not match `/` in filenames. To match files in subdirectories, include `/` in the pattern:

```shell,no_run
$ echo */*.txt
docs/readme.txt notes/todo.txt
```

Brackets cannot contain `/` because patterns are recognized for each component separated by `/`. For example, the pattern `a[/]b` only matches the literal pathname `a[/]b`, not `a/b`, because the brackets are considered unmatched in the sub-patterns `a[` and `]b`.

### Hidden files

By default, glob patterns do not match files starting with a dot (`.`). To match hidden files, the pattern must start with a literal dot:

```shell,no_run
$ echo .*.txt
.hidden.txt
```

<!-- TODO: dotglob option -->

Glob patterns never match the filenames `.` and `..`, even if a pattern begins with a literal dot.

```shell,no_run
$ echo .*
.backup.log .hidden.txt
```

## No matches

If a pattern does not match any files, it is left unchanged. If the shell does not have permission to read the directory, the pattern is also left unchanged.
<!-- TODO: nullglob option -->

## Summary

- Globbing expands patterns to matching pathnames.
- See [Pattern matching](../../patterns.md) for pattern syntax and details.
- Quote or escape glob characters to use them literally.
- Patterns that match nothing are left unchanged.
