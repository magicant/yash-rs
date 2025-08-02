# Alias built-in

The **`alias`** built-in defines [aliases](../language/aliases.md) or prints alias definitions.

## Synopsis

```sh
alias [name[=value]…]
```

## Description

The `alias` built-in defines aliases or prints existing alias definitions, depending on the operands. With no operands, it prints all alias definitions in a quoted assignment form suitable for reuse as input to `alias`.

## Options

None.

Non-POSIX options may be added in the future.

## Operands

Each operand must be of the form `name=value` or `name`. The first form defines an alias named *name* that expands to *value*. The second form prints the definition of the alias named *name*.

## Errors

It is an error if an operand without `=` refers to an alias that does not exist.

## Exit status

Zero unless an error occurs.

## Examples

See [Aliases](../language/aliases.md).

## Compatibility

The `alias` built-in is specified by POSIX.1-2024.

Some shells have predefined aliases that are printed even if you have not defined any explicitly.
