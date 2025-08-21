# Exit status and conditionals

This section describes the exit status of commands and how to use it to control the flow of execution in the shell.

## Exit status

The **exit status** of a command is a number that indicates an abstract result of the command's execution. It is used to determine whether a command succeeded or failed.

The value of the exit status is a non-negative integer, typically in the range of 0 to 255. The exit status is stored in the [special parameter] `?` immediately after a command runs.

```shell
$ test -f /nonexistent/file
$ echo $? # the exit status of `test`
1
$ echo $? # the exit status of the previous `echo`
0
```

Before running a command, the initial value of `?` is 0.

While the exact meaning of exit status values is specific to each command, there are some common conventions:

- An exit status of 0 indicates success.
- A non-zero exit status indicates failure or an error condition. The specific value can provide additional information about the type of failure.
- Exit statuses in the range of 1 to 125 are generally used by commands to indicate various types of errors or conditions.
- Exit statuses 126 and greater are reserved by the shell for special purposes.

The following exit statuses are used by the shell to indicate specific conditions:

- Exit status 126 indicates that a command was found but could not be executed.
- Exit status 127 indicates that a command was not found.
- Exit status 128 indicates that the shell encountered an unrecoverable error reading a command.
- When a command is terminated by a signal, the exit status is 384 plus the signal number. For example, if a command is terminated by `SIGINT` (signal number 2), the exit status will be 386.

### Compatibility of exit statuses

POSIX specifies exit statuses for certain conditions, but there are still many conditions for which POSIX does not define exact exit statuses. Different shells and commands may use different exit statuses for the same conditions, so it's important to check the documentation of the specific command you are using. Specifically, the exit status of a command terminated by a signal may vary between shells as POSIX only specifies that the exit status must be greater than 128.

Yash-rs internally handles exit statuses as 32-bit signed integers, but receives only the lower 8 bits from child processes running a [subshell](../../environment/index.html#subshells) or [external utility](simple.md#command-search). This means that exit statuses that are not in the range of 0 to 255 are truncated to fit into this range. For example, an exit status of 256 becomes 0, and an exit status of 1000 becomes 232.

### Exit status of the shell

When [exiting a shell](../../termination.md), the exit status of the shell itself is determined by the exit status of the last command executed in the shell. If no commands have been executed, the exit status is 0.

If the exit status of the last command indicates that the command was terminated by a signal, the shell sends the same signal to itself to terminate. The parent process (which may or may not be a shell) will observe that the shell process was terminated by a signal, allowing it to handle the termination appropriately. Specifically, if the parent process is also yash, the value of the [special parameter] `?` in the child shell process is reproduced in the parent shell process without modification.

This signal-passing behavior is not supported by all shells; in shells that do not support it, the lower 8 bits of the exit status are passed to the parent process instead. The parent process is likely to interpret this as an ordinary exit status, which may not accurately reflect the original command's termination by a signal.

## The `true` and `false` utilities

The [`true`](../../builtins/true.md) and [`false`](../../builtins/false.md) utilities simply return an exit status of 0 and 1, respectively. They are often used as placeholders in conditional statements or loops. See the examples in the [And-or lists](#and-or-lists) section below.

<!-- TODO: ## The `test` utility -->
<!-- TODO: ## The double bracket command -->

## Inverting exit status

You can invert a command's exit status using the `!` [reserved word]. This treats a successful command as a failure, and vice versa.

```shell
$ test -f /nonexistent/file
$ echo $?
1
$ ! test -f /nonexistent/file
$ echo $?
0
```

See [Negation](pipelines.md#negation) for more details.

## And-or lists

An **and-or list** is a sequence of commands that are executed based on the success or failure of previous commands. It allows you to control the flow of execution based on the exit status of commands.
An and-or list consists of commands separated by `&&` (and) or `||` (or) operators. The `&&` operator executes the next command only if the previous command succeeded (exit status 0), while the `||` operator executes the next command only if the previous command failed (non-zero exit status).

```shell
$ test -f /nonexistent/file && echo "File exists" || echo "File does not exist"
File does not exist
```

Unlike many other programming languages, the `&&` and `||` operators have equal precedence with left associativity in the shell language:

```shell
$ false && echo foo || echo bar
bar
$ { false && echo foo; } || echo bar
bar
$ false && { echo foo || echo bar; }
```

```shell
$ true || echo foo && echo bar
bar
$ { true || echo foo; } && echo bar
bar
$ true || { echo foo && echo bar; }
```

The `&&` and `||` operators can be followed by linebreaks for readability:

```shell
$ test -f /nonexistent/file &&
> echo "File exists" ||
> echo "File does not exist"
File does not exist
```

[Line continuation](../words/quoting.md#line-continuation) can also be used to split and-or lists across multiple lines:

```shell
$ test -f /nonexistent/file \
> && echo "File exists" \
> || echo "File does not exist"
File does not exist
```

The exit status of an and-or list is the exit status of the last command executed in the list.

## If commands

An **if command** is a conditional command that executes a block of commands based on the exit status of a test command. It allows you to perform different actions depending on whether a condition is true or false.

The minimal form of an if command uses the `if`, `then`, and `fi` [reserved words] that surround commands:

```shell,hidelines=#
#$ mkdir $$ && cd $$ && > foo.txt > bar.txt || exit
$ if diff -q foo.txt bar.txt; then echo "Files are identical"; fi
Files are identical
```

For readability, each reserved word can be on a separate line:

```shell,hidelines=#
#$ mkdir $$ && cd $$ && > foo.txt > bar.txt || exit
$ if diff -q foo.txt bar.txt
> then
>     echo "Files are identical"
> fi
Files are identical
```

You can also use the `elif` [reserved word] to add additional conditions:

```shell
$ if [ -f /dev/tty ]; then
>     echo "/dev/tty is a regular file"
> elif [ -d /dev/tty ]; then
>     echo "/dev/tty is a directory"
> elif [ -c /dev/tty ]; then
>     echo "/dev/tty is a character device"
> fi
/dev/tty is a character device
```

The `else` [reserved word] can be used to provide a default action if none of the conditions are met:

```shell
$ file=/nonexistent/file
$ if [ -e "$file" ]; then
>     echo "$file exists"
> elif [ -L "$file" ]; then
>     echo "$file is a symbolic link to a nonexistent file"
> else
>     echo "$file does not exist"
> fi
/nonexistent/file does not exist
```

The exit status of an if command is the exit status of the last command executed in the `then` or `else` clause. If no condition is met and there is no `else` clause, the exit status is 0 (success).

For repeating commands depending on a condition, see [While and until loops](loops.md#while-and-until-loops).

## Exiting on errors

By default, the shell continues running commands even if one fails (returns a non-zero exit status). This can cause later commands to run when they shouldn't. If you enable the `errexit` [shell option], the shell will exit immediately when any command fails, stopping further execution.

```shell
$ set -o errexit # or: set -e
$ test -e /dev/null
$ echo "Ok, continuing..."
Ok, continuing...
$ test -e /nonexistent/file
$ echo "This will not be printed"
```

In this example, after `test -e /nonexistent/file` fails, the shell exits right away, so you won't see any more prompts or output.

The `errexit` option only applies to the result of [pipelines](pipelines.md). It is ignored in these cases:

- When the pipeline is negated with the `!` [reserved word].
- When the pipeline is the left side of an [`&&` or `||`](#and-or-lists) operator.
- When the pipeline is part of the condition in an [`if` command](#if-commands) or a [`while` or `until` loop](loops.md#while-and-until-loops).

Although `errexit` does not catch every error, it is recommended for scripts to avoid unexpected results from failed commands. To skip `errexit` for a specific command, append `&& true`:

```shell
$ set -o errexit
$ test -e /nonexistent/file && true
$ echo "The exit status was $?"
The exit status was 1
```

<!-- TODO: ## The `errreturn` option -->

[reserved word]: ../words/keywords.md
[reserved words]: ../words/keywords.md
[shell option]: ../../environment/options.md
[special parameter]: ../parameters/special.md
