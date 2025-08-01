# Unset built-in

The **`unset`** built-in unsets shell variables or functions.

## Synopsis

```sh
unset [-fv] name...
```

## Description

The built-in unsets shell variables or functions named by the operands.

## Options

Either of the following options may be used to select what to unset:

- The **`-v`** (**`--variables`**) option causes the built-in to unset shell variables.
  This is the default behavior.
- The **`-f`** (**`--functions`**) option causes the built-in to unset shell functions.

(TODO: The `-l` (`--local`) option causes the built-in to unset local variables only.)

## Operands

Operands are the names of shell variables or functions to unset.

## Errors

Unsetting a read-only variable or function is an error.

It is not an error to unset a variable or function that is not set.
The built-in ignores such operands.

## Exit status

Zero unless an error occurs.

## Portability

The behavior is not portable when both `-f` and `-v` are specified. Earlier
versions of yash used to honor the last specified option, but this version
errors out.

If neither `-f` nor `-v` is specified and the variable named by an operand
is not set, POSIX allows the built-in to unset the same-named function if it
exists. Yash does not do this.

(TODO TBD: In the POSIXly-correct mode, the built-in requires at least one operand.)

When a global variable is hidden by a local variable, the current
implementation unsets the both. This is not portable. Old versions of yash
used to unset the local variable only.
