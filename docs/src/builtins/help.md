# Help built-in

(Since 3.6.0) The **`help`** built-in prints the help of built-ins.

## Synopsis

```sh
help [name…]
```

## Description

The built-in prints the help of the built-ins named by the operands to the standard output. The help of each built-in begins with a line consisting of the name of the built-in and a one-line summary, followed by the usage, the list of options, and a link to the manual page of the built-in. If more than one operand is given, the help of each built-in is separated by an empty line.

Some built-ins print the same help when invoked with the [`--help` option](index.html#help-option).

Not all built-ins have help yet.

If no operand is given, the built-in prints a short description of the shell and a list of the available built-ins with their summaries, sorted by name. A built-in that has no help is listed with `(no help available)` in place of its summary.

## Options

The **`--help`** option prints the [help](index.html#help-option) of the built-in.

## Operands

Each operand is the name of a built-in whose help should be printed. If no operand is given, the built-in lists the available built-ins as described above.

## Errors

It is an error if an operand does not name a built-in that has help. The built-in still prints the help of the other operands.

The [`portable` option](../environment/options.md#portable) causes additional errors; see [Compatibility](#compatibility).

## Exit Status

Zero if the help of all the operands, or the list of built-ins if no operand is given, was printed successfully; non-zero otherwise.

## Examples

```shell
$ help pwd
pwd: print the working directory path

Usage:
  pwd [-L|-P]

Options:
  -L, --logical   print $PWD if it is correct (default)
  -P, --physical  print the path without symbolic links
      --help      print this help

See <https://magicant.github.io/yash-rs/builtins/pwd.html> for details.
```

## Compatibility

The `help` built-in is not specified by POSIX. POSIX.1-2024 reserves the name `help` for a shell-specific built-in, and other shells implement it differently. For example, bash treats each operand as a pattern matching the names of built-ins.

The `help` built-in is an [elective built-in](index.html#elective-built-ins). When the [`portable` option](../environment/options.md#portable) is set, executing it is rejected with an error.
