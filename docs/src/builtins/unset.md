# Unset built-in

The **`unset`** built-in unsets [variables](../language/parameters/variables.md) or [functions](../language/functions.md).

## Synopsis

```sh
unset [-f|-v] name…
```

## Description

The built-in unsets variables or functions named by the operands.

## Options

Either of the following options may be used to select what to unset:

- The **`-v`** (**`--variables`**) option unsets variables.
  This is the default behavior.
- The **`-f`** (**`--functions`**) option unsets functions.

<!-- TODO: The `-l` (`--local`) option causes the built-in to unset local variables only. -->

## Operands

Operands are the names of variables or functions to unset.

## Errors

Unsetting a read-only variable or function is an error.

It is not an error to unset a variable or function that is not set.
The built-in ignores such operands.

## Exit status

Zero if successful; non-zero if an error occurs.

## Examples

See [Removing variables](../language/parameters/variables.md#removing-variables) and [Removing functions](../language/functions.md#removing-functions).

## Compatibility

The `unset` built-in is specified by POSIX.1-2024.

The behavior is not portable when both `-f` and `-v` are specified. Earlier versions of yash used to honor the last specified option, but this version errors out.

If neither `-f` nor `-v` is specified and the variable named by an operand is not set, POSIX allows the built-in to unset the same-named function if it exists. Yash does not do this.

POSIX requires that at least one operand be specified. Yash-rs currently does not detect this and allows the built-in to be called without any operands, but this may change in the future.

When a global variable is hidden by a local variable, the current implementation unsets the both. This is not portable. Old versions of yash used to unset the local variable only.
