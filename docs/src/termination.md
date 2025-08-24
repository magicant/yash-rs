# Termination

A shell session terminates in the following cases:

- When the shell reaches the end of input.
- When you use the [`exit` built-in](builtins/exit.md).
- When the shell receives a signal that causes it to terminate, such as `SIGINT` or `SIGTERM`, and no trap is set to handle that signal.
- When a non-[interactive shell](interactive/index.html) is interrupted by a [shell error](#shell-errors).
- When a command fails and the `errexit` [shell option] is enabled. (See [Exiting on errors](language/commands/exit_status.md#exiting-on-errors).)

## Preventing accidental exits

When the input to the shell is a terminal, you can signal an end-of-file with the `eof` sequence (usually `Ctrl+D`). However, you might not want the shell to exit immediately when this happens, especially if you often hit the sequence by mistake. Enable the `ignoreeof` [shell option] to prevent the shell from exiting on end-of-file and let it wait for more input.

```shell,no_run
$ set -o ignoreeof
$ 
# Type `exit` to leave the shell when the ignore-eof option is on.
$ exit
```

This option is only effective in [interactive shells](interactive/index.html) and only when the input is a terminal. As an escape, entering 50 eof sequences in a row will still cause the shell to exit, regardless of the `ignoreeof` option.

## Exiting subshells

When one of the above conditions occurs in a [subshell](environment/index.html#subshells), the subshell exits. It does not directly cause the parent shell to exit, but the [exit status] of the subshell may affect the parent shell's behavior, conditionally causing it to exit if the `errexit` option is set.

## `EXIT` trap

You can set a [trap](environment/traps.md) for the `EXIT` condition to run commands when the shell exits. This can be useful for cleanup tasks or logging. The trap is executed regardless of how the shell exits, whether due to an error, end-of-file, or explicit `exit` command, except when the shell is killed by a signal, in which case the trap is not executed (in yash-rs).

```shell,one_shot
$ trap 'rm -f temporary.txt; echo "Temporary file removed."' EXIT
$ echo "Some data" > temporary.txt
$ cat temporary.txt
Some data
$ exit
Temporary file removed.
```

The `EXIT` trap is run at most once per shell session. Modifying the `EXIT` trap while it is running does not have any effect on trap execution.

## Exit status

If the shell exits due to end of input, the `exit` built-in, or the `errexit` option, it returns the exit status of the last command executed. See [Exit status of the shell](language/commands/exit_status.md#exit-status-of-the-shell) for details.

If the shell exits because of a [shell error], the exit status is a non-zero value indicating the error.

## Shell errors

The following **shell errors** set the [exit status] to a non-zero value and may cause the shell to exit, depending on the situation:

- Unrecoverable errors reading input
    - The shell exits immediately.
    - This does not apply to scripts read by the [`source` built-in](builtins/source.md).
- Command syntax errors
    - The shell exits if non-interactive.
    - If interactive, the shell ignores the current command and resumes reading input.
- Errors in [special built-in utilities](builtins/index.html#special-built-ins)
    - The shell exits if non-interactive or if the `errexit` option is set. Otherwise, it aborts the current command and resumes reading input.
    - This includes [redirection](language/redirections/index.html) errors for [special built-ins](builtins/index.html#special-built-ins).
    - This does not apply to special built-ins run via the [`command` built-in](builtins/command.md).
- [Variable assignment](language/parameters/variables.md#defining-variables) errors and [expansion](language/words/index.html#word-expansion) errors
    - The shell exits if non-interactive or if `errexit` is set. Otherwise, it aborts the current command and resumes reading input.
- [Redirection](language/redirections/index.html) errors (except for [special built-ins](builtins/index.html#special-built-ins))
    - The shell exits if `errexit` is set. Otherwise, it continues with the next command.

POSIX.1-2024 allows shells to exit on [command search](language/commands/simple.md#command-search) errors, but many shells, including yash-rs, do not.

[exit status]: #exit-status
[shell error]: #shell-errors
[shell option]: environment/options.md
