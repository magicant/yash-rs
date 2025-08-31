# Pattern-based branching

The **case command** performs [pattern matching](../../patterns.md) on a value and executes commands for the first matching pattern. This is useful for branching logic based on specific values or patterns.

## Case command basics

A `case` command begins with the `case` [reserved word](../words/keywords.md), followed by the value to match. After the `in` reserved word, each branch specifies a pattern in parentheses, followed by a block of commands. Each block ends with `;;`, and the command ends with `esac`.

For example, this command matches the value of `foo` and runs the corresponding commands:

```shell
$ case foo in
> (foo)
>     echo "Matched foo"
>     ;;
> (bar)
>     echo "Matched bar"
>     ;;
> esac
Matched foo
```

## Patterns

[Patterns](../../patterns.md) can use wildcards and bracket expressions for flexible matching. For example, to match any string starting with `f`:

```shell
$ case foo in
> (f*)
>     echo "Starts with f"
>     ;;
> (b*)
>     echo "Starts with b"
>     ;;
> esac
Starts with f
```

To match multiple patterns, separate them with a pipe `|`:

```shell
$ case foo in
> (foo|bar)
>     echo "Matched foo or bar"
>     ;;
> esac
Matched foo or bar
```

## Word expansion

Both the value and patterns undergo [word expansion](../words/index.html#word-expansion):

- [Tilde expansion](../words/tilde.md)
- [Parameter expansion](../words/parameters.md)
- [Command substitution](../words/command_substitution.md)
- [Arithmetic expansion](../words/arithmetic.md)
- [Quote removal](../words/quoting.md#quote-removal) (applies only to the value)

```shell
$ value="Hello" pattern="[Hh]*"
$ case $value in
> ($pattern)
>     echo "Matched pattern"
>     ;;
> esac
Matched pattern
```

The value is always expanded first. Patterns are expanded only when the shell needs to match them. Once a pattern matches, remaining patterns are not expanded.

[Quote](../words/quoting.md) special characters in values or patterns to avoid unwanted expansion or matching:

```shell
$ case ? in
> ('?')
>     echo "Matched a single question mark"
>     ;;
> (?)
>     echo "Matched any single character"
>     ;;
> esac
Matched a single question mark
```

## Continuing to the next branch

Instead of `;;`, use `;&` to continue execution with the next branch, regardless of whether its pattern matches. This allows multiple branches to run in sequence:

```shell
$ case foo in
> (foo)
>     echo "Matched foo"
>     ;&
> (bar)
>     echo "Matched bar, or continued from foo"
>     ;;
> (baz)
>     echo "Matched baz"
>     ;;
> esac
Matched foo
Matched bar, or continued from foo
```

Use `;;&` or `;|` to continue pattern matching in subsequent branches, so commands in multiple matching branches can run:

```shell
$ case foo in
> (foo)
>     echo "Matched foo"
>     ;;&
> (bar)
>     echo "Matched bar"
>     ;;
> (f*)
>     echo "Matched any string starting with f"
>     ;;
> esac
Matched foo
Matched any string starting with f
```

The `;;&` and `;|` terminators are extensions to POSIX. yash-rs supports both, but other shells may support only one or neither.

## Miscellaneous

If no branch matches, or there are no branches, the shell skips the `case` command without error.

Use `*` as a catch-all pattern:

```shell
$ case foo in
> (bar)
>     echo "Matched bar"
>     ;;
> (*)
>     echo "Matched anything else"
>     ;;
> esac
Matched anything else
```

Use `''` or `""` as an empty value or pattern:

```shell
$ case "" in
> ('')
>     echo "Matched empty string"
>     ;;
> esac
Matched empty string
```

The opening parenthesis `(` can be omitted if the first pattern is not literally `esac`, but parentheses are recommended for clarity:

```shell
$ case foo in
> foo)
>     echo "Matched foo"
>     ;;
> esac
Matched foo
```

Branches can have empty command blocks:

```shell
$ case bar in
> (foo)
>     ;;
> (bar)
>     echo "Matched bar"
>     ;;
> esac
Matched bar
```

The `;;` terminator can be omitted for the last branch:

```shell
$ case foo in
> (foo)
>     echo "Matched foo"
>     ;;
> (bar)
>     echo "Matched bar"
> esac
Matched foo
```

## Exit status

The [exit status](exit_status.md#exit-status) of `case` is that of the last command executed in the last executed branch. If the last executed branch has no commands, or no pattern matches, the exit status is 0.

## Formal syntax

The formal syntax of the `case` command, in [Extended Backus-Naur Form (EBNF)](https://en.wikipedia.org/wiki/Extended_Backus%E2%80%93Naur_form):

```ebnf
case_command := "case", word, { newline }, "in", { newline },
                { branch }, [ last_branch ], "esac";
newline      := "\n";
branch       := pattern_list, branch_body, terminator, { newline };
last_branch  := pattern_list, branch_body;
pattern_list := "(", word, { "|" , word }, ")"
              | (word - "esac"), { "|" , word }, ")";
branch_body  := { newline }, [ list, [ newline, branch_body ] ];
terminator   := ";;" | ";&" | ";;&" | ";|";
```
