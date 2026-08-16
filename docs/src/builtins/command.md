# Command built-in

The **`command`** built-in executes a utility bypassing [functions].
This is useful when you want to execute a utility that has the same name as a function. The built-in also has options to search for the location of the utility.

## Synopsis

```sh
command [-p] name [arguments…]
```

```sh
command -v|-V [-p] name
```

## Description

Without the `-v` or `-V` option, the `command` built-in executes the utility specified by the *name* with the given *arguments*. This is similar to the execution of a [simple command](../language/commands/simple.md), but the functions are not searched for the *name*.

With the `-v` or `-V` option, the built-in identifies the type of the *name*
and, optionally, the location of the utility. The `-v` option prints the
pathname of the utility, if found, and the `-V` option prints a more
detailed description of the utility.

## Options

The **`-p`** (**`--path`**) option causes the built-in to search for the utility in the
standard search path instead of the current [`PATH`](../language/parameters/variables.md#reserved-variable-names).

The **`-v`** (**`--identify`**) option identifies the type of the command name and prints the
pathname of the utility, if found.

The **`-V`** (**`--verbose-identify`**) option identifies the type of the command name and prints a
more detailed description of the utility.

## Operands

The ***name*** operand specifies the name of the utility to execute or
identify. The ***arguments*** are passed to the utility when executing it.

## Standard output

When the `-v` option is given, the built-in prints the following:

- The absolute pathname of the utility, if found in the search path.
- The utility name itself, if it is a non-[substitutive](index.html#substitutive-built-ins) built-in, [function],
  or [reserved word](../language/words/keywords.md), hence not subject to search.
- A command line that would redefine the [alias](../language/aliases.md), if the name is an alias.

When the `-V` option is given, the built-in describes the utility in a more
descriptive, human-readable format. The exact format is not specified here
as it is subject to change.

Nothing is printed if the utility is not found.

## Errors

It is an error if the specified utility is not found or cannot be executed.

With the `-v` option, no error message is printed for the utility not found.

The [`portable` option](../environment/options.md#portable) causes additional errors; see [Compatibility](#compatibility).

## Exit status

Without the `-v` or `-V` option, the exit status is that of the utility
executed. If the utility is not found, the exit status is 127. If the
utility is found but cannot be executed, the exit status is 126.

With the `-v` or `-V` option, the exit status is 0 if the utility is found
and 1 if not found.

## Examples

See [Replacing existing utilities](../language/functions.md#replacing-existing-utilities).

## Compatibility

The long option names (`--path`, `--identify`, `--verbose-identify`) are a non-standard extension.

(Since 3.3.5) When the [`portable` option](../environment/options.md#portable) is set, using a long option name is rejected with an error. Use the corresponding short option instead.

POSIX requires that the *name* operand be specified, but many
implementations allow it to be omitted, in which case the built-in does
nothing. This implementation does so by default. (Since 3.3.5) The
[`portable` option](../environment/options.md#portable) rejects the omission
with an error.

POSIX specifies the `-v` and `-V` options as mutually exclusive, but many implementations allow both to be specified, with varying behavior. By default, yash honors the last one specified. (Since 3.3.5) The [`portable` option](../environment/options.md#portable) rejects the combination with an error.

POSIX allows only one *name* operand with the `-v` or `-V` option, but this
implementation identifies every operand given. (Since 3.3.5) The
[`portable` option](../environment/options.md#portable) rejects a surplus
operand with an error. Note that the
[`type` built-in](type.md), for which POSIX specifies `type name…`, accepts
more than one operand regardless of the option.

(Since 3.3.4) When the [`portable` option](../environment/options.md#portable)
is set, the built-in refuses to execute an
[elective or extension built-in](index.html#elective-built-ins), as well as the
[`.` built-in](source.md) invoked under the name `source`. For such a built-in,
the `-v` option produces no output and the `-V` option reports why it cannot be
used instead of describing it.

When the utility is not found with the `-v` or `-V` option, some
implementations return a non-zero exit status other than 1, especially 127.

When the utility is not found with the `-V` option, some implementations
print an error message to the standard output while others to the standard
error.

[function]: ../language/functions.md
[functions]: ../language/functions.md
