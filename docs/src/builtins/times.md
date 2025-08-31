# Times built-in

The **`times`** built-in is used to display the accumulated user and system
times for the shell and its children.

## Synopsis

```sh
times
```

## Description

The built-in prints the accumulated user and system times for the shell and
its children.

## Options

None.

## Operands

None.

## Standard output

Two lines are printed to the standard output, each in the following format:

```text
1m2.345678s 3m4.567890s
```

The first field of each line is the user time, and the second field is the system time.
The first line shows the times consumed by the shell itself, and the second line shows the times consumed by its child processes.

## Errors

It is an error if the times cannot be obtained or the standard output is not
writable.

## Exit status

Zero unless an error occurred.

## Compatibility

The `times` built-in is defined in POSIX.

POSIX does not require the `times` built-in to conform to the [Utility Syntax Guidelines](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap12.html#tag_12_02), which means portable scripts cannot use any options or the `--` separator for the built-in.

POSIX requires each field to be printed with six digits after the decimal
point, but many implementations print less. Note that the number of digits
does not necessarily indicate the precision of the times.
