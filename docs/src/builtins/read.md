# Read built-in

The **`read`** built-in reads a line into [variables](../language/parameters/variables.md).

## Synopsis

```sh
read [-d delimiter] [-r] variable…
```

## Description

The `read` built-in reads a line from the [standard input](../language/redirections/index.html#what-are-file-descriptors) and assigns it to the variables named by the operands. [Field splitting](../language/words/field_splitting.md) is performed on the line read to produce as many fields as there are variables. If there are fewer fields than variables, the remaining variables are set to empty strings. If there are more fields than variables, the last variable receives all remaining fields, including the field separators, but not trailing whitespace separators.

### Non-default delimiters

By default, the built-in reads a line up to a newline character. The `-d` option changes the delimiter to the character specified by the `delimiter` value. If the `delimiter` value is empty, the built-in reads a line up to the first nul byte.

### Escaping

By default, backslashes in the input are treated as [quoting](../language/words/quoting.md) characters that prevent the following character from being interpreted as a field separator. Backslash-newline pairs are treated as [line continuations](../language/words/quoting.md#line-continuation).

The `-r` option disables this behavior.

### Prompting

By default, the built-in does not display a prompt before reading a
line. (TODO: Options to display a prompt)

When reading lines after the first line, the built-in displays the value of the `PS2` [variable](../language/parameters/variables.md) as a prompt if the shell is [interactive](../interactive/index.html) and the input is from a terminal.

## Options

The **`-d`** (**`--delimiter`**) option takes an argument and changes the
delimiter to the character specified by the argument. If the `delimiter`
value is empty, the `read` built-in reads a line up to the first nul byte.
Multibyte characters are not supported.

The **`-r`** (**`--raw-mode`**) option disables the interpretation of
backslashes.

## Operands

One or more operands are required.
Each operand is the name of a variable to be assigned.

## Errors

This built-in fails if:

- The standard input is not readable.
- The delimiter is not a single-byte character.
- The delimiter is not a nul byte and the input contains a nul byte.
- A variable name is not valid.
- A variable to be assigned is read-only.

## Exit status

The exit status is zero if a line was read successfully and non-zero
otherwise. If the built-in reaches the end of the input before finding a
delimiter, the exit status is one, but the variables are still assigned with
the line read so far. On other errors, the exit status is two or higher.

## Compatibility

The `read` built-in is defined in the POSIX standard with the `-d` and `-r` options.

In this implementation, a line continuation is always a backslash followed by a newline. Other implementations may allow a backslash followed by a delimiter to be a line continuation if the delimiter is not a newline.

When a backslash is specified as the delimiter, no escape sequences are recognized. Other implementations may recognize escape sequences in the input line, effectively never recognizing the delimiter.

In this implementation, the value of the `PS2` variable is subject to [parameter expansion](../language/words/parameters.md), [command substitution](../language/words/command_substitution.md), and [arithmetic expansion](../language/words/arithmetic.md). Other implementations may not perform these expansions.

The current implementation considers variable names containing a `=` as invalid names. However, more names many be considered invalid in the future. For best forward-compatibility and portability, only use portable name characters (ASCII alphanumerics and underscore).
