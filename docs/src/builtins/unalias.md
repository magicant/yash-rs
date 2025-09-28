# Unalias built-in

The **`unalias`** built-in removes [alias](../language/aliases.md) definitions.

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

## Examples

```shell
$ alias greet='echo Hello,'
$ greet world
Hello, world
$ unalias greet
$ greet world
error: cannot execute external utility "greet"
 --> <stdin>:4:1
  |
4 | greet world
  | ^^^^^ utility not found
```

## Compatibility

The `unalias` built-in is specified in POSIX.

Some shells implement some built-in utilities as predefined aliases. Using `unalias -a` may make such built-ins unavailable.
