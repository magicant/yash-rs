# Alias built-in

The **`alias`** built-in defines aliases or prints alias definitions.

## Synopsis

```sh
alias name…
```

## Description

The alias built-in defines aliases as specified by the operands. If no operands
are given, the built-in prints all alias definitions.

## Options

None.

## Operands

Each operand must be the name of an alias to define. If an operand contains an
equal sign (`=`), the operand is split into the name and value at the first
equal sign. The value is assigned to the alias named by the name. Otherwise,
the alias named by the operand is printed.

## Errors

It is an error if an operand names a non-existent alias when printing.

## Exit status

Zero unless an error occurs.

## Portability

The alias built-in is specified in POSIX.