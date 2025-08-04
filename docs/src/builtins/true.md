# True built-in

The **`true`** built-in command does nothing, successfully.

## Synopsis

```sh
true
```

## Description

The `true` built-in command does nothing, successfully. It is useful as a
placeholder when a command is required but no action is needed.

## Options

None.

## Operands

None.

## Errors

None.

(TODO: In the future, the built-in may detect unexpected options or operands.)

## Exit Status

Zero.

## Compatibility

Most implementations ignore any arguments, but some implementations may
accept them. For example, the GNU coreutils implementation accepts the
`--help` and `--version` options. For maximum portability, avoid passing
arguments to the `true` command.
