# Continue built-in

The **`continue`** built-in skips the execution of a loop to the next
iteration.

## Synopsis

```sh
continue [n]
```

## Description

`continue n` interrupts the execution of the *n*th innermost for, while, or
until loop and resumes its next iteration.
The specified loop must lexically enclose the continue command, that is:

- The loop is running in the same execution environment as the continue
  command; and
- The continue command appears inside the condition or body of the loop but
  not in the body of a function definition command appearing inside the
  loop.

It is an error if there is no loop enclosing the continue command.
If *n* is greater than the number of enclosing loops, the built-in affects
the outermost one.

## Options

None.

(TODO: the -i option)

## Operands

Operand *n* specifies the nest level of the affected loop.
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

POSIX allows the built-in to restart a loop running in the current execution
environment that does not lexically enclose the continue command.
Our implementation declines to do that.
