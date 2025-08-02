# Set built-in

The **`set`** built-in modifies [shell options](yash_env::option) and
[positional parameters](yash_env::variable). It also can print a list of
current options or variables.

## Description

The built-in behaves differently depending on the invocation syntax.

### Printing variables

```sh
set
```

When executed without any arguments, the built-in prints a list of
[variables](yash_env::variable) visible in the current execution
environment. The list is formatted as a sequence of simple commands
performing an assignment that would restore the present variables if
executed (unless the assignment fails because of a read-only variable).
The list is ordered alphabetically.

Some built-ins allow creating variables that do not have a valid name. For
example, `export 1a=foo` defines a variable named `1a`, which cannot be
assigned to with the normal assignment syntax of the simple command.
Such variables are not printed by the `set` built-in.

### Printing options

```sh
set -o
```

If you specify the `-o` option as a unique argument to the set built-in, it
prints the current option settings in a human-readable format.

```sh
set +o
```

If you use the `+o` option instead, the printing lists shell commands that
would restore the current option settings if executed.

### Modifying shell options

Other options modify [shell option](yash_env::option::Option) settings. They
can be specified in the short form like `-e` or the long form like `-o
errexit` and `--errexit`.

You can also specify options starting with `+` in place of `-`, as in `+e`,
`+o errexit`, and `++errexit`. The `-` options turn on the corresponding
shell options while the `+` options turn off.

See [`parse_short`] for a list of available short options and [`parse_long`]
to learn how long options are parsed.
Long options are [canonicalize]d before being passed to `parse_long`.

You cannot modify the following options with the set built-in:

- `CmdLine` (`-c`, `-o cmdline`)
- `Interactive` (`-i`, `-o interactive`)
- `Stdin` (`-s`, `-o stdin`)

### Modifying positional parameters

If you specify one or more operands, they will be new positional parameters
in the current [context](yash_env::variable), replacing any existing
positional parameters.

### Option-operand separator

As with other utilities conforming to POSIX XBD Utility Syntax Guidelines,
the set built-in accepts `--` as a separator between options and operands.
Additionally, you can separate them with `-` instead of `--`.

If you place a separator without any operands, the built-in will clear all
positional parameters.

## Exit status

- 0: successful
- 1: error printing output
- 2: invalid options

## Portability

POSIX defines only the following option names:

- `-a`, `-o allexport`
- `-b`, `-o notify`
- `-C`, `-o noclobber`
- `-e`, `-o errexit`
- `-f`, `-o noglob`
- `-h`
- `-m`, `-o monitor`
- `-n`, `-o noexec`
- `-u`, `-o nounset`
- `-v`, `-o verbose`
- `-x`, `-o xtrace`

Other options (including non-canonicalized ones) are not portable. Also,
using the `no` prefix to negate an arbitrary option is not portable. For
example, `+o noexec` is portable, but `-o exec` is not.

The output format of `set -o` and `set +o` depends on the shell.

The semantics of `-` as an option-operand separator is unspecified in POSIX.
You should prefer `--`.

Many (but not all) shells specially treat `+`, especially when it appears in
place of an option-operand separator. This behavior is not portable either.
