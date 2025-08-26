# Continue built-in

The **`continue`** built-in skips the execution of a [loop] to the next iteration.

## Synopsis

```sh
continue [n]
```

## Description

`continue n` interrupts the execution of the *n*th innermost `for`, `while`, or `until` [loop] and resumes its next iteration.  The specified loop must lexically enclose the continue command, that is:

- The loop is running in the same [execution environment] as the continue command; and
- The continue command appears inside the condition or body of the loop but not in the body of a [function definition command](../language/functions.md#defining-functions) appearing inside the loop.

It is an error if there is no loop enclosing the continue command.
If *n* is greater than the number of enclosing loops, the built-in affects
the outermost one.

If the affected loop is a `for` loop, the loop variable is updated to the next value in the list. The loop ends if there are no more values to iterate over.

If the affected loop is a `while` or `until` loop, the condition is re-evaluated.

## Options

None.

(TODO: the `-i` option)

## Operands

Operand ***n*** specifies the nest level of the affected loop.
If omitted, it defaults to 1. It is an error if the value is not a positive
decimal integer.

## Exit status

Zero if the built-in successfully continues the loop; non-zero otherwise.

## Examples

See [Break and continue](../language/commands/loops.md#break-and-continue).

## Compatibility

The behavior is unspecified in POSIX when the continue built-in is used
without an enclosing loop, in which case the current implementation returns
an error.

POSIX allows the built-in to restart a loop running in the current [execution environment] that does not lexically enclose the continue command. Our implementation declines to do that.

POSIX does not require the `continue` built-in to conform to the [Utility Syntax Guidelines](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap12.html#tag_12_02), which means portable scripts cannot use any options or the `--` separator for the built-in.

[execution environment]: ../environment/index.html
[loop]: ../language/commands/loops.md
