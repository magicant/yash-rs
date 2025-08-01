# False built-in

The **`false`** built-in command does nothing, unsuccessfully.

## Synopsis

```sh
false
```

## Description

The `false` built-in command does nothing and returns a non-zero exit
status.

## Options

None.

## Operands

None.

## Errors

None.

(TODO: In the future, the built-in may detect unexpected options or operands.)

## Exit Status

[`ExitStatus::FAILURE`].

## Portability

POSIX allows the `false` built-in to return any non-zero exit status, but
most implementations return one.

Most implementations ignore any arguments, but some implementations may
accept them. For example, the GNU coreutils implementation accepts the
`--help` and `--version` options. For maximum portability, avoid passing
arguments to the `false` command.
