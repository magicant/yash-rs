# Startup

This section describes how yash-rs is started and configured.

## Command-line arguments

Start the shell by running the `yash3` executable. The general syntax is:

```sh
yash3 [options] [file [arguments…]]
yash3 [options] -c command [command_name [arguments…]]
yash3 [options] -s [arguments…]
```

The shell's behavior is determined by the options and operands you provide.

### Options

The shell accepts [shell options] to control its behavior. The following options are only available at startup:

- `-c` (`--cmdline`): Read and execute commands from the `command` operand.
- `-s` (`--stdin`): Read and execute commands from standard input.
- `-i` (`--interactive`): Force the shell to be interactive.
- `-l` (`--login`): Make the shell a login shell. This can also be triggered by a leading hyphen in the command name (e.g., `-yash3`).
- `--profile <file>`: Specify a profile file to execute.
- `--noprofile`: Do not execute any profile file.
- `--rcfile <file>`: Specify an rcfile to execute.
- `--norcfile`: Do not execute any rcfile.

### Modes of operation

The shell has three modes:

- **File mode:** If neither `-c` nor `-s` is specified, the first operand is treated as the path to a script file to execute. Any following operands become [positional parameters] for the script.
- **Command string mode:** With `-c`, the shell executes the command string given as the first operand. If `command_name` is specified, it sets the [special parameter] `0`. Remaining operands become [positional parameters].
- **Standard input mode:** With `-s`, the shell reads commands from standard input. Any operands are set as positional parameters.

If no operands are given and `-c` is not specified, the shell assumes `-s`.

## Initialization files

When the shell starts, it may execute one or more initialization files to configure the environment.

### Login shell

If the shell is a login shell (started with `-l` or a leading hyphen in its name), it executes a profile file. The path can be set with `--profile`. Use `--noprofile` to skip the profile file.

⚠️ Profile file execution is not yet implemented.

### Interactive shell

If the shell is interactive, it executes an rcfile. The path can be set with `--rcfile`. Use `--norcfile` to skip the rcfile.

If no rcfile is specified, the shell checks the `ENV` [environment variable]. If set, its value is expanded for [parameter expansion], [command substitution], and [arithmetic expansion], and used as the rcfile path.

The rcfile is only executed if:

- The shell is interactive.
- The real user ID matches the effective user ID.
- The real group ID matches the effective group ID.

## Compatibility

Options for initialization files (`--profile`, `--noprofile`, `--rcfile`, `--norcfile`) are not part of POSIX.1-2024 and may not be available in other shells. See [Compatibility](environment/options.md#compatibility) in the options documentation for portable shell options.

POSIX.1-2024 does not specify login shells or profile files. The behavior described here is specific to yash-rs and may differ from other shells.

Using the `ENV` [environment variable] for initialization files is POSIX-specified. In the future, yash-rs may support a different default rcfile location depending on a shell option.

[arithmetic expansion]: language/words/arithmetic.md
[command substitution]: language/words/command_substitution.md
[environment variable]: language/parameters/variables.md#environment-variables
[parameter expansion]: language/words/parameters.md
[positional parameters]: language/parameters/positional.md
[shell options]: environment/options.md
[special parameter]: language/parameters/special.md
