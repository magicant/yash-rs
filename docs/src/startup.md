# Startup

This section describes how yash-rs is started and configured.

## Command-line arguments

Start the shell by running the `yash3` executable. The general syntax is:

```sh
yash3 [options] [file [arguments…]]
yash3 [options] -c command [command_name [arguments…]]
yash3 [options] -s [arguments…]
```

The shell's behavior is determined by the options and operands you provide. See [Command line argument syntax conventions](builtins/index.html#command-line-argument-syntax-conventions) for how arguments are parsed into options and operands.

### Options

The shell accepts [shell options] to control its behavior. The following options are only available at startup:

`-c` (`--cmdline`)
: Read and execute commands from the `command` operand.

`-s` (`--stdin`)
: Read and execute commands from [standard input].

`-i` (`--interactive`)
: Force the shell to be [interactive].

`-l` (`--login`)
: Make the shell a login shell. This can also be triggered by a leading hyphen in the command name (e.g., `-yash3`).

`--profile <file>`
: Specify a [profile file] to execute.

`--noprofile`
: Do not execute any [profile file].

`--rcfile <file>`
: Specify an [rcfile] to execute.

`--norcfile`
: Do not execute any [rcfile].

### Modes of operation

The shell has three modes:

File mode
: If neither `-c` nor `-s` is specified, the first operand is treated as the path to a script file to execute. Any following operands become [positional parameters] for the script.

Command string mode
: With `-c`, the shell executes the command string given as the first operand. If `command_name` is specified, it sets the [special parameter] `0`. Remaining operands become [positional parameters].

Standard input mode
: With `-s`, the shell reads commands from [standard input]. Any operands are set as positional parameters.

If no operands are given and `-c` is not specified, the shell assumes `-s`.

## Initialization files

When the shell starts, it may execute one or more initialization files to configure the environment.

### Login shell

If the shell is a login shell (started with `-l` or a leading hyphen in its name), it executes a profile file. The path can be set with `--profile`. Use `--noprofile` to skip the profile file.

⚠️ Profile file execution is not yet implemented.

### Interactive shell

If the shell is [interactive], it executes an rcfile. The path can be set with `--rcfile`. Use `--norcfile` to skip the rcfile.

If no rcfile is specified, the shell checks the `ENV` [environment variable]. If set, its value is expanded for [parameter expansion], [command substitution], and [arithmetic expansion], and used as the rcfile path.

The rcfile is only executed if:

- The shell is interactive,
- The real user ID matches the effective user ID, and
- The real group ID matches the effective group ID.

## Compatibility

Options for initialization files (`--profile`, `--noprofile`, `--rcfile`, `--norcfile`) are not part of POSIX.1-2024 and may not be available in other shells. See [Compatibility](environment/options.md#compatibility) in the options documentation for portable shell options.

(Since 3.3.5) When the [`portable` option](environment/options.md#portable) is enabled on the command line, the options that follow it must be spelled the way POSIX specifies. This rejects the `--name` and `++name` forms, abbreviated and case-insensitive `-o` names, the `-l` option, the `+c` and `+s` negations (POSIX specifies `-c` and `-s`, but not their negated forms), and the yash-specific `-V`, `--help`, `--version`, `--profile`, `--rcfile`, `--noprofile`, and `--norcfile` options. Write those before `-o portable` if you need both.

```sh
yash3 --rcfile myrc -o portable myscript   # accepted
yash3 -o portable --rcfile myrc myscript   # rejected
```

(Since 3.3.5) The [POSIX Utility Argument Syntax](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap12.html#tag_12_01) requires a conforming application to write an option argument as a separate argument. When the `portable` option is enabled on the command line, the shell rejects the argument to `-o` or `+o` written in the same argument as the option itself, as in `yash3 -oerrexit`. Write `yash3 -o errexit` instead.

(Since 3.4.1) POSIX allows a single `-` to mark the end of the options, just as `--` does, but leaves the results undefined if both are given. When the `portable` option is enabled on the command line, the shell rejects `-` and `--` given together, as in `yash3 - -- myscript`.

POSIX also leaves the results undefined if other operands precede a single `-`, as in `yash3 myscript -`. Yash-rs treats such a `-` as an ordinary operand, as other shells do, and the `portable` option does not reject or warn about it.

(Since 3.3.3) POSIX only requires the shell to support [environment variable]s whose names consist of ASCII letters, digits, and underscores and do not start with a digit. When the `portable` option is enabled on the command line, the shell ignores inherited environment variables with other names instead of importing them, so that a script cannot rely on them.

POSIX.1-2024 does not specify login shells or profile files. The behavior described here is specific to yash-rs and may differ from other shells.

Using the `ENV` [environment variable] for initialization files is POSIX-specified. In the future, yash-rs may support a different default rcfile location depending on the command name and shell options.

[arithmetic expansion]: language/words/arithmetic.md
[command substitution]: language/words/command_substitution.md
[environment variable]: language/parameters/variables.md#environment-variables
[interactive]: interactive/index.html
[parameter expansion]: language/words/parameters.md
[positional parameters]: language/parameters/positional.md
[profile file]: #login-shell
[rcfile]: #interactive-shell
[shell options]: environment/options.md
[special parameter]: language/parameters/special.md
[standard input]: language/redirections/index.html#what-are-file-descriptors
