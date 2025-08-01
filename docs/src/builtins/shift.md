# Shift built-in

The **`shift`** built-in removes some positional parameters.

## Synopsis

```sh
shift [n]
```

## Description

The built-in removes the first *n* positional parameters from the list of
positional parameters. If *n* is omitted, it is assumed to be `1`.

## Options

None. (TBD: non-portable extensions)

## Operands

The operand specifies the number of positional parameters to remove. It must
be a non-negative decimal integer less than or equal to the number of
positional parameters.

## Errors

It is an error to try to remove more than the number of existing positional
parameters.

## Exit status

Zero unless an error occurs.

## Portability

POSIX does not specify whether an invalid operand is a syntax error or a
runtime error. This implementation treats it as a syntax error.

(TODO: the array option and negative operands)
