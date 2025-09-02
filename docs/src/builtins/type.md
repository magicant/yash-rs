# Type built-in

The **`type`** built-in identifies the type of commands.

## Synopsis

```sh
type [name…]
```

## Description

The `type` built-in prints the description of the specified command names.

## Options

None.

<!-- TODO: Non-standard options -->

## Operands

The ***name*** operands specify the command names to identify.

## Standard output

The command descriptions are printed to the standard output.

## Errors

It is an error if the *name* is not found.

## Exit status

The exit status is zero if all the *name*s are found, and non-zero
otherwise.

## Examples

```shell,hidelines=#
#$ PATH=/usr/bin:/bin
$ alias ll='ls -l'
$ greet() { echo "Hello, world!"; }
$ type ll greet cd env
ll: alias for `ls -l`
greet: function
cd: mandatory built-in
env: external utility at /usr/bin/env
```

## Compatibility

The `type` built-in is specified by POSIX.1-2024.

POSIX defines no options for the `type` built-in, but previous versions of yash supported additional options, which are not yet implemented in yash-rs.

POSIX does not require the `type` built-in to conform to the [Utility Syntax Guidelines](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap12.html#tag_12_02), which means portable scripts cannot use any options or the `--` separator for the built-in.

POSIX requires that the *name* operand be specified, but many implementations allow it to be omitted, in which case the built-in does nothing.

The format of the output is unspecified by POSIX. In this implementation, the `type` built-in is equivalent to the [`command` built-in](command.md) with the `-V` option.
