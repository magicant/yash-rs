# Type built-in

The **`type`** built-in identifies the type of commands.

## Synopsis

```sh
type [name…]
```

## Description

The `type` built-in prints the description of the specified command names.

## Options

(TODO: Non-standard options are not supported yet.)

## Operands

The ***name*** operands specify the command names to identify.

## Standard output

The command descriptions are printed to the standard output.

## Errors

It is an error if the *name* is not found.

## Exit status

The exit status is zero if all the *name*s are found, and non-zero
otherwise.

## Compatibility

POSIX requires that the *name* operand be specified, but many
implementations allow it to be omitted, in which case the built-in does
nothing.

The format of the output is unspecified by POSIX. In this implementation,
the `type` built-in is equivalent to the [`command`] built-in with the `-V`
option.

[`command`]: crate::command
