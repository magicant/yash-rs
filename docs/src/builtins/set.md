# Set built-in

The **`set`** built-in modifies [shell options](../environment/options.md) and [positional parameters](../language/parameters/positional.md). It also can print a list of current options or [variables](../language/parameters/variables.md).

## Description

The built-in behaves differently depending on the invocation syntax.

### Printing variables

When executed without any arguments, the built-in prints a list of [variables](../language/parameters/variables.md) visible in the current execution environment. The list is formatted as a sequence of simple commands performing an assignment that would restore the present variables if executed (unless the assignment fails because of a read-only variable). The list is ordered alphabetically.

```shell,no_run
$ set
HOME=/home/alice
OPTIND=1
LOGNAME=alice
PATH=/usr/local/bin:/usr/bin:/bin
PWD=/home/alice/my_project
TERM=xterm
```

Some built-ins allow creating variables that do not have a valid name. For example, `export 1a=foo` defines a variable named `1a`, which cannot be assigned to with the normal assignment syntax of the simple command. Such variables are not printed by the `set` built-in.

### Printing shell options

If you specify the `-o` option as a unique argument to the set built-in, it
prints the current [shell option](../environment/options.md) settings in a human-readable format:

```shell,no_run
$ set -o
allexport        off
clobber          on
cmdline          off
errexit          off
exec             on
...
```

If you use the `+o` option instead, the printing lists shell commands that
would restore the current option settings if executed:

```shell,no_run
$ set +o
set +o allexport
set -o clobber
#set +o cmdline
set +o errexit
set -o exec
...
```

### Modifying shell options

Other command line options modify [shell option](../environment/options.md) settings. They can be specified in the short form like `-e` or the long form like `-o errexit` and `--errexit`.

You can also specify options starting with `+` in place of `-`, as in `+e`, `+o errexit`, and `++errexit`. The `-` options turn on the corresponding shell options while the `+` options turn off.

See [Enabling and disabling options](../environment/options.md#enabling-and-disabling-options) for the full details on the option syntax. Available options are listed in the [Option list](../environment/options.md#option-list).

You cannot modify the following options with the `set` built-in:

- `cmdline` (`-c`)
- `interactive` (`-i`)
- `stdin` (`-s`)

### Modifying positional parameters

If you specify one or more operands, they will be new [positional parameters](../language/parameters/positional.md) in the current [shell environment](../environment/index.html), replacing any existing positional parameters.

See [Modifying positional parameters](../language/parameters/positional.md#modifying-positional-parameters) for examples of how to set positional parameters.

### Option-operand separator

As with other utilities conforming to POSIX XBD Utility Syntax Guidelines,
the set built-in accepts `--` as a separator between options and operands.
Additionally, you can separate them with `-` instead of `--`.

```shell
$ set -o errexit -- foo bar
$ echo "$1" "$2"
foo bar
```

If you place a separator without any operands, the built-in will clear all
positional parameters.

```shell
$ set --
$ echo $#
0
```

## Exit status

- 0: successful
- 1: error printing output
- 2: invalid options

## Compatibility

See [Compatibility](../environment/options.md#compatibility) for the compatibility of the option syntax and available options.

The output format of `set -o` and `set +o` depends on the shell.

The semantics of `-` as an option-operand separator is unspecified in POSIX. You should prefer `--`.

Many (but not all) shells specially treat `+`, especially when it appears in
place of an option-operand separator. Yash does not treat `+` specially, so it can be used as an operand without another separator.

Other implementations may return different non-zero exit statuses for errors.
