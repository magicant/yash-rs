# Signals and traps

Signals are a method of inter-process communication used to notify a process that a specific event has occurred. The POSIX standard defines a set of signals that have well-defined meanings and behaviors across Unix-like systems. Traps are the shell's mechanism for handling signals and other events by executing custom commands when specific conditions occur.

## What are signals?

**Signals** are asynchronous notifications sent to a process to inform it of an event. Common signals include:

- `SIGINT`: Interrupt signal, typically sent when the user presses `Ctrl+C`
- `SIGTERM`: Termination request, used to ask a process to exit gracefully
- `SIGQUIT`: Quit signal, typically sent when the user presses `Ctrl+\`
- `SIGHUP`: Hangup signal, originally sent when a terminal connection was lost
- `SIGKILL`: Kill signal that cannot be caught or ignored
- `SIGSTOP`: Stop signal that cannot be caught or ignored
- `SIGTSTP`: Terminal stop signal, typically sent when the user presses `Ctrl+Z`
- `SIGCHLD`: Child process terminated or stopped
- `SIGUSR1` and `SIGUSR2`: User-defined signals for custom applications

Available signals may vary by system. For a complete list, refer to your system's documentation or use `kill -l`.

When a process receives a signal, it can respond in one of three ways:

1. **Default action**: Follow the system's default behavior for that signal (usually termination)
2. **Ignore**: Do nothing when the signal is received
3. **Custom action**: Execute a custom signal handler

## What are traps?

**Traps** are the shell's way of defining custom responses to signals and other events. When you set a trap, you specify:

- A **condition** that triggers the trap (such as a signal or shell exit), and
- An **action** to perform when the condition occurs.

The shell checks for pending signals and executes corresponding trap actions at safe points during execution, typically before and after executing commands. This ensures that trap actions run in a consistent shell state.

### Special conditions

In addition to signals, the shell supports the [`EXIT` condition](../termination.md#exit-trap), which is triggered when the shell exits (but not when killed by a signal). This allows you to run cleanup commands or perform other actions when the shell session ends.

More conditions may be supported in future versions of the shell.

### Trap inheritance and subshells

When the shell creates a [subshell](index.html#subshells):

- Traps set to ignore are inherited by the subshell.
- Traps with custom actions are reset to default behavior.

External utilities inherit the signal dispositions from the shell, but not custom trap actions.

## Setting traps

Use the `trap` built-in to configure traps or view current traps.

### Restrictions

- `SIGKILL` and `SIGSTOP` cannot be caught or ignored.
- If a non-interactive shell inherited an ignored signal, that signal cannot be trapped. Interactive shells can modify signals that were initially ignored.

## How and when traps are executed

Signal traps run when signals are caught.

- When a signal is caught while the shell is running a command, the shell waits for the command to finish before executing the trap action.
- If a signal is caught while the shell is reading input, the shell waits for the input to complete before executing the trap action. This behavior may change in future versions so that traps can run immediately.
- While executing a signal trap action, other signal traps are not processed (no reentrance), except in subshells.

`EXIT` traps run when the shell exits normally, after all other commands complete.

The [exit status](../language/commands/exit_status.md) is preserved across trap action execution, but trap actions can use the `exit` built-in to terminate the shell with a specific exit status.

## Auto-ignored signals

In an [interactive shell](../interactive.md), certain signals are automatically ignored by default to prevent the shell from being terminated or stopped unintentionally. Specifically:

- `SIGINT`, `SIGTERM`, and `SIGQUIT` are always ignored.
- If job control is enabled, `SIGTSTP`, `SIGTTIN`, and `SIGTTOU` are also ignored.

This ensures the shell remains responsive and in control, even if these signals are sent. You can still set traps for these signals if needed. In [subshells](index.html#subshells), which are non-interactive, this automatic ignoring does not apply.
