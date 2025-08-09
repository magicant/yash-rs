# Umask built-in

The **`umask`** built-in shows or sets the file mode creation mask.

## Synopsis

```sh
umask [-S] [mode]
```

## Description

The built-in shows the current file mode creation mask if no *mode* is
given. Otherwise, it sets the file mode creation mask to *mode*.

## Options

The **`-S`** (**`--symbolic`**) option causes the built-in to show the
current file mode creation mask in symbolic notation.

## Operands

*mode* is an octal integer or a symbolic notation that represents the file
mode creation mask. The octal number is the bitwise OR of the file mode bits
to be turned off when creating a file. The symbolic notation specifies the
file mode bits to be kept on when creating a file. The symbolic notation
consists of one or more clauses separated by commas. Each clause consists of
a (possibly empty) sequence of who symbols followed by one or more actions.
The who symbols are:

- **`u`** for the user bits,
- **`g`** for the group bits,
- **`o`** for the other bits, and
- **`a`** for all bits.

An action is an operator optionally followed by permission symbols. The
operators are:

- **`+`** to add the permission,
- **`-`** to remove the permission, and
- **`=`** to set the permission.

The permission symbols are:

- one or more of:
    - **`r`** for the read permission,
    - **`w`** for the write permission,
    - **`x`** for the execute permission,
    - **`X`** for the execute permission if the execute permission is
      already set for any who, and
    - **`s`** for the set-user-ID-on-execution and set-group-ID-on-execution
      bits.
- **`u`** for the current user permission,
- **`g`** for the current group permission, and
- **`o`** for the current other permission.

For example, the symbolic notation `u=rwx,go+r-w`

- sets the user bits to read, write, and execute,
- adds the read permission to the group and the other bits, and
- removes the write permission from the group and the other bits.

## Standard output

If no *mode* is given, the built-in prints the current file mode creation
mask in octal notation followed by a newline to the standard output.
If the `-S` option is effective, the mask is formatted in symbolic notation
instead.

## Errors

It is an error if the specified *mode* is not a valid file mode creation
mask.

## Exit status

Zero unless an error occurred.

## Compatibility

The `umask` built-in is defined in POSIX.

POSIX does not specify the default output format used when the `-S` option is
not given. Our implementation, as well as many others, uses octal notation.

This implementation ignores the `-S` option if *mode* is given. However,
bash prints the new mask in symbolic notation if the `-S` option and *mode*
are both given.

An empty sequence of who symbols is equivalent to `a` in this implementation
as well as many others. However, this may not be strictly true to the POSIX
specification.

The permission symbols other than `r`, `w`, and `x` are not widely supported.
This implementation currently ignores the `s` symbol.