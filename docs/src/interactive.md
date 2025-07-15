# Interactive shell

When the `interactive` [shell option](environment/options.md) is enabled, the shell behaves in a way that is more suitable for interactive use. Currently, only the essential features are implemented, but more will be added in the future.

## Enabling interactive mode

When you start the shell without arguments in a terminal, it usually enables interactive mode by default:

```sh
yash3
```

Specifically, interactive mode is enabled if:

- you do not specify `+i`,
- [the `-s` option is active, either explicitly or implicitly](startup.md#modes-of-operation), and
- [standard input and standard error](language/redirections/index.html#what-are-file-descriptors) are terminals.

To force the shell to be interactive, use the `-i` option:

```sh
yash3 -i
```

Interactive mode can only be set at startup. To change the interactive mode, you must restart the shell.

## Telling if the shell is interactive

To determine if the shell is running in interactive mode, check whether the `-` [special parameter] contains `i`:

```sh
case $- in
  *i*) echo "Interactive shell" ;;
  *)   echo "Non-interactive shell" ;;
esac
```

See [Viewing current options](environment/options.md#viewing-current-options) for additional methods.

## What happens in interactive mode

When the shell is interactive:

- The shell executes an [rcfile](startup.md#interactive-shell) during startup.
- The `-` [special parameter] includes `i`.
- The `exec` [shell option](environment/options.md) is always considered set.
- The `ignoreeof` [shell option](environment/options.md) is honored.
- [Starting an asynchronous command prints its job number and process ID](language/commands/lists.md#asynchronous-commands).
- The shell does not exit immediately on most [shell errors](termination.md#shell-errors).
- [Some signals are automatically ignored](environment/traps.md#auto-ignored-signals).
- [Signals ignored on entry can be trapped](environment/traps.md#restrictions).
- Command prompts are displayed when reading input.
- Job status changes are reported before prompting for input if job control is enabled.
- The `read` built-in displays a prompt when reading a second or subsequent line of input.

[special parameter]: language/parameters/special.md
