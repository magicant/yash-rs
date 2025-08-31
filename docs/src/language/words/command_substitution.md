# Command substitution

**Command substitution** expands to the output of a command. It has two forms: the preferred `$(command)` form and the deprecated backquote form `` `command` ``.

For example, this runs `dirname -- "$0"` and passes its output to `cd`:

<!-- markdownlint-disable MD014 -->
```shell
$ cd -P -- "$(dirname -- "$0")"
```
<!-- markdownlint-enable MD014 -->

This changes the [working directory](../../environment/working_directory.md) to the directory containing the script, regardless of the current directory.

## Syntax

The `$(…)` form evaluates the command inside the parentheses. It supports nesting and is easier to read than backquotes:

```shell
$ echo $(echo $(echo hello))
hello
$ echo "$(echo "$(echo hello)")"
hello
```

In the backquote form, backslashes [escape](quoting.md#backslash) `$`, `` ` ``, and `\`. If backquotes appear inside double quotes, backslashes also escape `"`. These escapes are processed before the command is run. A backquote-form equivalent to the previous example is:

```shell
$ echo `echo \`echo hello\``
hello
$ echo "`echo \"\`echo hello\`\"`"
hello
```

The `$(…)` form can be confused with [arithmetic expansion](arithmetic.md). Command substitution is only recognized if the code is not a valid arithmetic expression. For example, `$((echo + 1))` is arithmetic expansion, but `$((echo + 1); (echo + 2))` is command substitution. To force command substitution starting with a [subshell](../commands/grouping.md#subshells), insert a space: `$( (echo + 1); (echo + 2))`.

## Semantics

The command runs in a subshell, and its [standard output](../redirections/index.html#what-are-file-descriptors) is captured. [Standard error](../redirections/index.html#what-are-file-descriptors) is not captured unless redirected. Trailing newlines are removed, and the result replaces the command substitution in the command line.

Currently, yash-rs parses the command when the substitution is executed, not when it is parsed. This may change in the future, affecting when syntax errors are detected and when [aliases](../aliases.md) are substituted.

## Compatibility

When a command substitution only contains [redirections](../redirections/index.html), some shells treat it specially. For example, `$(<file)` is equivalent to `$(cat file)` in bash and zsh, but yash-rs does not support this yet.
