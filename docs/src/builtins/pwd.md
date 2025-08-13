# Pwd built-in

The **`pwd`** built-in prints the [working directory] path.

## Synopsis

```sh
pwd [-L|-P]
```

## Description

The built-in prints the pathname of the current [working directory] followed by a newline to the standard output.

## Options

With the **`-L`** (**`--logical`**) option, the printed path is the value of the `PWD` [variable] if it is correct. The path may contain symbolic link components, but not `.` or `..` components.

With the **`-P`** (**`--physical`**) option (or if `PWD` is not correct), the built-in recomputes and prints the actual path to the working directory. The output excludes symbolic link components as well as `.` and `..` components.

These two options are mutually exclusive. The last specified one applies if given both. The default is `-L`.

## Operands

None.

## Errors

This built-in may fail for various reasons. For example:

- The working directory has been removed from the file system.
- You lack permission to access one or more ancestor directories required to determine the working directory’s path.
- The standard output is not writable.

## Exit Status

Zero if the path was successfully printed; non-zero otherwise.

## Compatibility

The `-L` and `-P` options are defined in POSIX.

POSIX allows the built-in to apply the `-P` option if the `-L` option is specified and `PWD` is longer than `PATH_MAX`.

The shell sets `PWD` on the [startup](../startup.md) and modifies it in the [`cd` built-in](cd.md). If `PWD` is modified or unset otherwise, the behavior of the `cd` and `pwd` built-ins is unspecified.

[variable]: ../language/parameters/variables.md
[working directory]: ../environment/working_directory.md
