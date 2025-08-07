# Shift built-in

The **`shift`** built-in removes some [positional parameters](../language/parameters/positional.md).

## Synopsis

```sh
shift [n]
```

## Description

The built-in removes the first *n* positional parameters from the list of
positional parameters. If *n* is omitted, it is assumed to be `1`.

## Options

None.

## Operands

The operand specifies the number of positional parameters to remove. It must
be a non-negative decimal integer less than or equal to the number of
positional parameters.

## Errors

It is an error to try to remove more than the number of existing positional
parameters.

## Exit status

Zero if successful, non-zero if an error occurred.

## Examples

[Modifying positional parameters](../language/parameters/positional.md#modifying-positional-parameters) includes an example of the `shift` built-in.

## Compatibility

The `shift` built-in is part of POSIX.1-2024.
