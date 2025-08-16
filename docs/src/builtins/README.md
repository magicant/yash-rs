# Built-in utilities

**Built-in utilities** (or **built-ins**) are utilities built into the shell, not separate executables. They run directly in the shell process, making them faster and more efficient for certain tasks.

## Types of built-in utilities

Yash provides several types of built-in utilities.

### Special built-ins

**Special built-ins** have special meaning or behavior in the shell. They are used for control flow, variable manipulation, and other core tasks. Notable properties:

- [Command search](../language/commands/simple.md#command-search) always finds special built-ins first, regardless of `PATH`.
- [Functions](../language/functions.md) cannot override special built-ins.
- Assignments in a simple command running a special built-in persist after the command.
- Errors in special built-ins cause the shell to exit if non-interactive.

POSIX.1-2024 defines these special built-ins:

- `.` (dot)
- `:` (colon)
- `break`
- `continue`
- `eval`
- `exec`
- `exit`
- `export`
- `readonly`
- `return`
- `set`
- `shift`
- `times`
- `trap`
- `unset`

As an extension, yash-rs also supports `source` as an alias for `.` (dot).

### Mandatory built-ins

**Mandatory built-ins** must be implemented by all POSIX-compliant shells. They provide essential scripting and command features.

Like special built-ins, they are found regardless of `PATH` in [command search](../language/commands/simple.md#command-search), but they can be overridden by functions.

POSIX.1-2024 defines these mandatory built-ins:

- `alias`
- `bg`
- `cd`
- `command`
- `fc` (not yet implemented)
- `fg`
- `getopts`
- `hash` (not yet implemented)
- `jobs`
- `kill`
- `read`
- `type`
- `ulimit`
- `umask`
- `unalias`
- `wait`

### Elective built-ins

**Elective built-ins** work like mandatory built-ins but are not required by POSIX.1-2024. They provide extra features for scripting or interactive use.

Elective built-ins can be overridden by functions and are found in [command search](../language/commands/simple.md#command-search) regardless of `PATH`.

In yash-rs, the following elective built-in is implemented:

- `typeset`

More may be added in the future.

### Substitutive built-ins

**Substitutive built-ins** replace external utilities to avoid process creation overhead for common tasks.

Substitutive built-ins behave like external utilities: they are located during [command search](../language/commands/simple.md#command-search) and can be overridden by [functions](../language/functions.md). However, the built-in is only available if the corresponding external utility exists in `PATH`. If the external utility is missing from `PATH`, the built-in is also unavailable, ensuring consistent behavior with the absence of the utility.

Yash-rs implements these substitutive built-ins:

- `false`
- `pwd`
- `true`

More may be added in the future.

### Compatibility

POSIX.1-2024 reserves many names for shell-specific built-ins. Yash-rs implements some of these, and may add more in the future. Other shells may implement these differently:

- `alloc`
- `autoload`
- `bind`
- `bindkey`
- `builtin`
- `bye`
- `caller`
- `cap`
- `chdir`
- `clone`
- `comparguments`
- `compcall`
- `compctl`
- `compdescribe`
- `compfiles`
- `compgen`
- `compgroups`
- `complete`
- `compound`
- `compquote`
- `comptags`
- `comptry`
- `compvalues`
- `declare`
- `dirs`
- `disable`
- `disown`
- `dosh`
- `echotc`
- `echoti`
- `enum`
- `float`
- `help`
- `hist`
- `history`
- `integer`
- `let`
- `local`
- `login`
- `logout`
- `map`
- `mapfile`
- `nameref`
- `popd`
- `print`
- `pushd`
- `readarray`
- `repeat`
- `savehistory`
- `shopt`
- `source`
- `stop`
- `suspend`
- `typeset`
- `whence`

## Command line argument syntax conventions

**Arguments** are string parameters passed to built-in utilities. The syntax varies between built-ins, but most follow common conventions.

### Operands

**Operands** are main arguments specifying objects or values for the built-in. For example, in `cd`, the operand is the directory:

```shell
$ cd /dev
```

### Options

**Options** are supplementary arguments that modify the behavior of the built-in. They start with a hyphen (`-`) followed by one or more characters. Short options are named with a single character (e.g., `-P`), while long options are more descriptive and start with two hyphens (e.g., `--physical`). For example, the `cd` built-in uses `-P` or `--physical` to force the shell to use the physical directory structure instead of preserving symbolic links:

```shell
$ cd -P /dev
```

With a long option:

```shell
$ cd --physical /dev
```

Multiple short options can be combined. For example, `cd -P -e /dev` can be written as:

```shell
$ cd -Pe /dev
```

Long options must be specified separately.

Long option names can be abbreviated if unambiguous. For example, `--p` is enough for `--physical` in `cd`:

```shell
$ cd --p /dev
```

However, future additions may make abbreviations ambiguous, so use the full name in scripts.

POSIX.1-2024 only specifies short option syntax. Long options are a yash-rs extension.

### Option arguments

Some options require an argument. For short options, the argument can follow immediately or as a separate argument. For example, `-d` in `read` takes a delimiter argument:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ echo 12 42 + foo bar > line.txt
$ read -d + a b < line.txt
$ echo "A: $a, B: $b"
A: 12, B: 42
```

If the argument is non-empty, it can be attached: `-d+`. If empty, specify separately: `-d ''`:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ echo 12 42 + foo bar > line.txt
$ read -d+ a b < line.txt
$ echo "A: $a, B: $b"
A: 12, B: 42
$ read -d '' a b < line.txt
$ echo "A: $a, B: $b"
A: 12, B: 42 + foo bar
```

For long options, use `=` or a separate argument:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ echo 12 42 + foo bar > line.txt
$ read --delimiter=+ a b < line.txt
$ echo "A: $a, B: $b"
A: 12, B: 42
$ read --delimiter + a b < line.txt
$ echo "A: $a, B: $b"
A: 12, B: 42
```

### Separators

To treat an argument starting with `-` as an operand, use the `--` separator. This tells the shell to stop parsing options. For example, to change to a directory named `-P`:

```shell,hidelines=#
#$ mkdir $$ $$/-P && cd $$ || exit
$ cd -- -P
```

Note that a single hyphen (`-`) is not an option, but an operand. It can be used without `--`:

```shell,hidelines=#
$ cd /tmp
$ cd /
$ cd -
/tmp
```

### Argument order

Operands must come after options. All arguments after the first operand are treated as operands, even if they start with a hyphen:

```shell
$ cd /dev -P
error: unexpected operand
 --> <stdin>:1:9
  |
1 | cd /dev -P
  | --      ^^ -P: unexpected operand
  | |
  | info: executing the cd built-in
  |
```

Specifying options after operands may be supported in the future.
