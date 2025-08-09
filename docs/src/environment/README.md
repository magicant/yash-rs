# Shell environment and subshells

The **shell execution environment** is the set of state the shell maintains to control its behavior. It consists of:

- [File descriptors](../language/redirections/index.html#what-are-file-descriptors)
- Working directory
- File creation mask <!-- TODO: link to umask -->
- Resource limits <!-- TODO: link to ulimit -->
- [Variables](../language/parameters/variables.md)
- [Positional parameters](../language/parameters/positional.md)
- Values of these [special parameters](../language/parameters/special.md):
    - `?`: [exit status](../language/commands/exit_status.md) of the last command
    - `$`: process ID of the shell
    - `!`: process ID of the last [asynchronous command]
    - `0`: name of the shell or script
- [Functions](../language/functions.md)
- [Aliases](../language/aliases.md)
- [Shell options](options.md)
- [Traps](traps.md)
- [Job list](../interactive/job_control.md#job-list)

## Subshells

A **subshell** is a separate environment created as a copy of the current shell environment. Changes in a subshell do not affect the parent shell. A subshell starts with the same state as the parent, except that traps with custom commands are reset to default behavior.

Create a subshell using [parentheses](../language/commands/grouping.md#subshells). Subshells are also created implicitly when running an [external utility](../language/commands/simple.md#command-search), a [command substitution](../language/words/command_substitution.md), an [asynchronous command], or a multi-command [pipeline](../language/commands/pipelines.md).

Subshells of an interactive shell are not themselves interactive, even if the `interactive` [option](options.md) is set.

Yash-rs currently implements subshells using the `fork` system call, which creates a new process. This may change in the future for greater efficiency.

[External utilities](../language/commands/simple.md#command-search) run by the shell inherit the following from the shell environment:

- File descriptors
- Working directory
- File creation mask
- Resource limits
- Environment variables
- Traps, except those with a custom command

[asynchronous command]: ../language/commands/lists.md#asynchronous-commands
