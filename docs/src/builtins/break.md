# Break built-in

The **`break`** built-in terminates the execution of a [loop].

## Synopsis

```sh
break [n]
```

## Description

`break n` quits the execution of the *n*th innermost `for`, `while`, or `until` [loop]. The specified loop must lexically enclose the break command, that is:

- The loop is running in the same [execution environment] as the break command; and
- The break command appears inside the condition or body of the loop but not in the body of a [function definition command](../language/functions.md#defining-functions) appearing inside the loop.

It is an error if there is no loop enclosing the break command.
If *n* is greater than the number of enclosing loops, the built-in exits the
outermost one.

## Options

None.

## Operands

Operand ***n*** specifies the nest level of the loop to exit.
If omitted, it defaults to 1.
It is an error if the value is not a positive decimal integer.

## Exit status

Zero if the built-in successfully breaks the loop; non-zero otherwise.

## Examples

See [Break and continue](../language/commands/loops.md#break-and-continue).

## Compatibility

The `break` built-in is specified by POSIX.1-2024.

The behavior is unspecified in POSIX when the `break` built-in is used without an enclosing loop, in which case the current implementation returns an error.

POSIX allows the built-in to break a loop running in the current [execution environment] that does not lexically enclose the break command. Our implementation does not do that.

POSIX does not require the `break` built-in to conform to the [Utility Syntax Guidelines](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap12.html#tag_12_02), which means portable scripts cannot use any options or the `--` separator for the built-in.

Previous versions of yash supported the non-standard `-i` option, but this is not yet supported in yash-rs.

[execution environment]: ../environment/index.html
[loop]: ../language/commands/loops.md
