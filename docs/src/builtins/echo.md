# Echo built-in

(Since 3.7.0) The **`echo`** built-in prints its arguments.

## Synopsis

```sh
echo [-n] [-e|-E] [STRING…]
```

## Description

The built-in prints the operands to the standard output, separated by a space and followed by a newline. Without operands, it prints only a newline.

By default, the built-in accepts no options and interprets the escape sequences described below. The [`ECHO_STYLE` variable](#echo-style) changes this behavior.

### Escape sequences

When escape sequences are enabled, the built-in replaces each of the following in the operands with the corresponding character:

| Escape sequence | Replacement |
|-----------------|-------------|
| `\a`            | Alert (bell) |
| `\b`            | Backspace |
| `\c`            | Nothing; the built-in prints nothing more, not even the trailing newline |
| `\e`            | Escape |
| `\f`            | Form feed |
| `\n`            | Newline |
| `\r`            | Carriage return |
| `\t`            | Horizontal tab |
| `\v`            | Vertical tab |
| `\\`            | Backslash |
| `\0nnn`         | The byte whose value is the octal number *nnn* of up to three digits |

If the value of `\0nnn` exceeds 255 (octal 377), only its lowest eight bits are used. Any other backslash is currently printed as is, but future versions may interpret more escape sequences.

### Echo style

The **`ECHO_STYLE`** [variable] selects which options the built-in accepts and whether it interprets escape sequences by default:

| `ECHO_STYLE` | Options accepted | Escape sequences |
|--------------|------------------|------------------|
| `SYSV` or `XSI` | None | Always enabled |
| `BSD`        | `-n`             | Never enabled |
| `GNU`        | `-n`, `-e`, `-E` | Disabled by default |
| `ZSH`        | `-n`, `-e`, `-E` | Enabled by default |
| `DASH`       | `-n`             | Always enabled |
| `RAW`        | None             | Never enabled |

Only the first character of the value is significant, and it is case-insensitive. If the variable is unset or has any other value, the built-in behaves as with `SYSV`.

## Options

The current [echo style](#echo-style) determines which of the following options the built-in accepts. An option that the style does not accept is treated as an operand. An argument is an option if it starts with `-` followed by one or more characters, each of which is an accepted option. The built-in examines arguments from the first one and treats them as options until it finds one that is not an option. A single `-` and `--` are operands. If an argument combines several options, as in `-ne`, the built-in applies each of them.

The **`-n`** option suppresses the trailing newline.

The **`-e`** option enables escape sequences.

The **`-E`** option disables escape sequences.

If both `-e` and `-E` are given, the last one applies.

## Operands

Each operand is a string to print.

## Errors

The built-in fails if it cannot write to the standard output.

The [`portable` option](../environment/options.md#portable) causes additional errors; see [Compatibility](#compatibility).

## Exit status

Zero if successful; non-zero if an error occurs.

## Examples

```shell
$ echo Hello, world!
Hello, world!
$ echo 'first line\nsecond line'
first line
second line
$ echo -n foo
-n foo
$ ECHO_STYLE=BSD echo -n 'no newline\n'; echo
no newline\n
```

## Compatibility

The `echo` utility is specified by POSIX.1-2024. POSIX leaves the results implementation-defined if the first operand is `-` followed by one or more of `e`, `E`, and `n`, or if any operand contains a backslash. The behavior of the `SYSV` style is what POSIX requires on XSI-conformant systems, except for the `\e` escape sequence, which is an extension. Implementations of `echo` in other shells differ widely in these cases, so use the [`printf` built-in](printf.md) instead for portable output.

The `ECHO_STYLE` variable is an extension inherited from yash 2. Many of its values are named after other shells, but the built-in does not exactly mimic them.

When the [`portable` option](../environment/options.md#portable) is set, the built-in rejects a first operand that is `-` followed by one or more of `e`, `E`, and `n`, and an operand that contains a backslash, regardless of the echo style. The built-in prints nothing in that case.

[variable]: ../language/parameters/variables.md
