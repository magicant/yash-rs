# Pwd built-in

The **`pwd`** built-in prints the working directory path.

## Synopsis

```sh
pwd [-L|-P]
```

## Description

The built-in prints the pathname of the current working directory followed
by a newline to the standard output.

## Options

With the **`-L`** (**`--logical`**) option, the printed path is the value of `$PWD` if it is correct. The path may contain symbolic link components, but not `.` or `..` components.

With the **`-P`** (**`--physical`**) option (or if `$PWD` is not correct),
the built-in recomputes and prints the canonical path to the working
directory.

These two options are mutually exclusive. The last specified one applies if
given both. The default is `-L`.

## Operands

None.

## Errors

This built-in may fail for various reasons. For example:

- The working directory has been removed from the file system.
- You do not have permission to access the ancestor directories of the working directory.
- The standard output is not writable.

## Exit Status

Zero if the path was successfully printed; non-zero otherwise.

## Portability

The `-L` and `-P` options are defined in POSIX.

POSIX allows the built-in to apply the `-P` option if the `-L` option is
specified and `$PWD` is longer than `PATH_MAX`.

The shell sets `$PWD` on the startup and modifies it in the [`cd` built-in](cd.md). If `$PWD` is modified or unset otherwise, the behavior of the `cd` and `pwd` built-ins is unspecified.
