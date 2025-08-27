# Eval built-in

The **`eval`** built-in evaluates the arguments as shell commands.

## Synopsis

```sh
eval [command…]
```

## Description

This built-in parses and executes the argument as a shell script in the current [shell environment](../environment/index.html).

## Options

None.

(TODO: non-portable options)

## Operands

The operand is a command string to be evaluated.
If more than one operand is given, they are concatenated with spaces
between them to form a single command string.

## Errors

During parsing and execution, any syntax error or runtime error may
occur.

## Exit status

The exit status of the `eval` built-in is the exit status of the last
command executed in the command string.
If there is no command in the string, the exit status is zero.
In case of a syntax error, the exit status is 2.

## Examples

See [Evaluating command strings](../dynamic_evaluation.md#evaluating-command-strings).

## Security considerations

The `eval` built-in can be dangerous if used with untrusted input, as it can execute arbitrary commands. It is recommended to avoid using `eval` with user input or to sanitize the input before passing it to `eval`.

## Compatibility

The `eval` built-in is specified by POSIX.1-2024.

POSIX does not require the `eval` built-in to conform to the [Utility Syntax Guidelines](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap12.html#tag_12_02), which means portable scripts cannot use any options or the `--` separator for the built-in.
