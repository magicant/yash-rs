# Command prompt

When an [interactive shell](index.html) reads input, it displays a **command prompt**—a string indicating that the shell is ready to accept commands. The prompt can be customized to display information such as the current [working directory], username, or hostname.

## Customizing the command prompt

The command prompt is controlled by the `PS1` and `PS2` [variables](../language/parameters/variables.md):

- `PS1` defines the primary prompt, shown when the shell is ready for a new command.
- `PS2` defines the secondary prompt, shown when the shell expects more input to complete a command (i.e., when a command spans multiple lines).

Each time the shell displays a prompt, it performs [parameter expansion](../language/words/parameters.md), [command substitution](../language/words/command_substitution.md), and [arithmetic expansion](../language/words/arithmetic.md) on the prompt strings. This allows prompts to include dynamic information, such as the [working directory] or username.

After these expansions, the shell performs exclamation mark expansion ([see below](#exclamation-mark-expansion)) on the `PS1` prompt. `PS2` is not subject to exclamation mark expansion.

The default values for these variables are:

```sh
PS1='$ '
PS2='> '
```

Many shells change the default `PS1` to `# ` for the root user, but yash-rs does not yet support this. <!-- markdownlint-disable-line MD038 -->

Custom prompts are usually set in the [rcfile](../startup.md#interactive-shell). For example, to include the username, hostname, and [working directory] in the prompt, add this to your rcfile:

```sh
PS1='${LOGNAME}@${HOSTNAME}:${PWD} $ '
```

You do not need to export `PS1` or `PS2` for them to take effect.

## Exclamation mark expansion

Exclamation mark expansion replaces an exclamation mark (`!`) in the `PS1` prompt with the history number of the next command. However, yash-rs does not yet support command history, so this feature is currently non-functional.

To include a literal exclamation mark in the prompt, use a double exclamation mark (`!!`).

## Compatibility

POSIX.1-2024 allows shells to perform exclamation mark expansion before other expansions, in which case exclamation marks produced by those expansions are not replaced.

Additional special notation that starts with a backslash (`\`), supported by earlier versions of yash, is not yet implemented in yash-rs.

[working directory]: ../environment/working_directory.md
