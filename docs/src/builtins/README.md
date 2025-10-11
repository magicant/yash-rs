# Built-in utilities

**Built-in utilities** (or **built-ins**) are utilities built into the shell, not separate executables. They run directly in the shell process, making them faster and more efficient for certain tasks.

## Types of built-in utilities

Yash provides several types of built-in utilities.

### Special built-ins

**Special built-ins** have special meaning or behavior in the shell. They are used for control flow, variable manipulation, and other core tasks. Notable properties:

- [Command search](../language/commands/simple.md#command-search) always finds special built-ins first, regardless of `PATH`.
- [Functions](../language/functions.md) cannot override special built-ins.
- Assignments in a [simple command](../language/commands/simple.md) running a special built-in persist after the command.
- Errors in special built-ins cause the shell to exit if non-interactive. (See [Shell errors](../termination.md#shell-errors))

POSIX.1-2024 defines these special built-ins:

- [`.` (dot)](source.md)
- [`:` (colon)](colon.md)
- [`break`](break.md)
- [`continue`](continue.md)
- [`eval`](eval.md)
- [`exec`](exec.md)
- [`exit`](exit.md)
- [`export`](export.md)
- [`readonly`](readonly.md)
- [`return`](return.md)
- [`set`](set.md)
- [`shift`](shift.md)
- [`times`](times.md)
- [`trap`](trap.md)
- [`unset`](unset.md)

As an extension, yash-rs also supports [`source`](source.md) as an alias for `.` (dot).

### Mandatory built-ins

**Mandatory built-ins** must be implemented by all POSIX-compliant shells. They provide essential scripting and command features.

Like [special built-ins](#special-built-ins), they are found regardless of `PATH` in [command search](../language/commands/simple.md#command-search), but they can be overridden by [functions].

POSIX.1-2024 defines these mandatory built-ins:

- [`alias`](alias.md)
- [`bg`](bg.md)
- [`cd`](cd.md)
- [`command`](command.md)
- `fc` (not yet implemented)
- [`fg`](fg.md)
- [`getopts`](getopts.md)
- `hash` (not yet implemented)
- [`jobs`](jobs.md)
- [`kill`](kill.md)
- [`read`](read.md)
- [`type`](type.md)
- [`ulimit`](ulimit.md)
- [`umask`](umask.md)
- [`unalias`](unalias.md)
- [`wait`](wait.md)

### Elective built-ins

**Elective built-ins** work like [mandatory built-ins](#mandatory-built-ins) but are not required by POSIX.1-2024. They provide extra features for scripting or interactive use.

Elective built-ins can be overridden by [functions] and are found in [command search](../language/commands/simple.md#command-search) regardless of `PATH`.

In yash-rs, the following elective built-in is implemented:

- [`typeset`](typeset.md)

More may be added in the future.

### Substitutive built-ins

**Substitutive built-ins** replace external utilities to avoid process creation overhead for common tasks.

Substitutive built-ins behave like external utilities: they are located during [command search](../language/commands/simple.md#command-search) and can be overridden by [functions]. However, the built-in is only available if the corresponding external utility exists in `PATH`. If the external utility is missing from `PATH`, the built-in is also unavailable, ensuring consistent behavior with the absence of the utility.

Yash-rs implements these substitutive built-ins:

- [`false`](false.md)
- [`pwd`](pwd.md)
- [`true`](true.md)

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
- [`source`](source.md)
- `stop`
- `suspend`
- [`typeset`](typeset.md)
- `whence`

## Command line argument syntax conventions

**Arguments** are string parameters passed to built-in utilities. The syntax varies between built-ins, but most follow common conventions. The description below applies to yash-rs built-ins unless otherwise noted.

### Operands

**Operands** are main arguments specifying objects or values for the built-in. For example, in [`cd`](cd.md), the operand is the directory:

<!-- markdownlint-disable MD014 -->
```shell
$ cd /dev
```
<!-- markdownlint-enable MD014 -->

### Options

**Options** are supplementary arguments that modify the behavior of the built-in. They start with a hyphen (`-`) followed by one or more characters. Short options are named with a single character (e.g., `-P`), while long options are more descriptive and start with two hyphens (e.g., `--physical`). For example, the [`cd` built-in](cd.md) uses `-P` or `--physical` to force the shell to use the physical directory structure instead of preserving symbolic links:

<!-- markdownlint-disable MD014 -->
```shell
$ cd -P /dev
```
<!-- markdownlint-enable MD014 -->

With a long option:

<!-- markdownlint-disable MD014 -->
```shell
$ cd --physical /dev
```
<!-- markdownlint-enable MD014 -->

Multiple short options can be combined. For example, `cd -P -e /dev` can be written as:

<!-- markdownlint-disable MD014 -->
```shell
$ cd -Pe /dev
```
<!-- markdownlint-enable MD014 -->

Long options must be specified separately.

Long option names can be abbreviated if unambiguous. For example, `--p` is enough for `--physical` in `cd`:

<!-- markdownlint-disable MD014 -->
```shell
$ cd --p /dev
```
<!-- markdownlint-enable MD014 -->

However, future additions may make abbreviations ambiguous, so use the full name in scripts.

POSIX.1-2024 only specifies short option syntax. Long options are a yash-rs extension.

### Option arguments

Some options require an argument. For short options, the argument can follow immediately or as a separate argument. For example, `-d` in [`read`](read.md) takes a delimiter argument:

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
  | while executing the cd built-in
```

Specifying options after operands may be supported in the future.

[functions]: ../language/functions.md
