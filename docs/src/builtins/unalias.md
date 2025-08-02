# Unalias built-in

The **`unalias`** built-in removes alias definitions.

## Synopsis

```sh
unalias name…
```

```sh
unalias -a
```

## Description

The unalias built-in removes alias definitions as specified by the operands.

## Options

The **`-a`** (**`--all`**) option removes all alias definitions.

## Operands

Each operand must be the name of an alias to remove.

## Errors

It is an error if an operand names a non-existent alias.

## Exit status

Zero unless an error occurs.

## Portability

The unalias built-in is specified in POSIX.

Some shells implement some built-in utilities as predefined aliases. Using
`unalias -a` may make such built-ins unavailable.