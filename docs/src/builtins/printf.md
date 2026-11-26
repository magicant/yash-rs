# Printf built-in

(Since 3.8.0) The **`printf`** built-in prints its arguments according to a format.

## Synopsis

```sh
printf [-v VARIABLE] FORMAT [ARGUMENT…]
```

A format that starts with `-` must be preceded by `--`, as in `printf -- '-%s' foo`.

## Description

The built-in formats the arguments according to the format operand and prints the result to the standard output. Unlike the [`echo` built-in](echo.md), it does not print a newline unless the format contains one.

The format is a string that may contain [conversion specifications](#conversion-specifications), which start with `%`, and [escape sequences](#escape-sequences), which start with `\`. The built-in prints any other characters in the format as they are.

Each conversion specification except `%%` consumes an argument and prints it in the specified form. Arguments are consumed in order unless conversion specifications select them by [position](#argument-positions). If arguments remain after the whole format has been processed, the built-in processes the format again to consume them, and repeats this until all arguments are consumed. If the format contains no conversion specification that consumes an argument, the built-in processes it only once and ignores the arguments.

If a conversion specification has no argument left to consume, the built-in uses an empty string for a string conversion and zero for a numeric conversion.

### Conversion specifications

A conversion specification consists of `%` followed by an optional [argument position](#argument-positions), optional [flags](#flags), an optional [field width](#field-width), an optional [precision](#precision), and a conversion specifier, in this order. The conversion specifier is one of the following characters:

| Specifier | Output |
|-----------|--------|
| `d`, `i`  | Signed integer in decimal |
| `u`       | Unsigned integer in decimal |
| `o`       | Unsigned integer in octal |
| `x`       | Unsigned integer in lowercase hexadecimal |
| `X`       | Unsigned integer in uppercase hexadecimal |
| `f`, `F`  | Floating-point number in the fixed-point notation, such as `123.456000` |
| `e`, `E`  | Floating-point number in the exponent notation, such as `1.234560e+02` |
| `g`, `G`  | Floating-point number in the notation of `f` or `e`, depending on the exponent |
| `c`       | First byte of the argument |
| `s`       | Argument as it is |
| `b`       | Argument with [escape sequences of the `echo` built-in](echo.md#escape-sequences) interpreted |
| `q`       | Argument [quoted](../language/words/quoting.md) so that it can be reused as shell input |
| `%`       | `%` itself, without consuming an argument |

The uppercase specifiers `F`, `E`, and `G` print `INF`, `NAN`, and the exponent marker `E` in uppercase. For `g` and `G`, the built-in uses the exponent notation if the exponent is less than -4 or not less than the precision. Unless the `#` flag is given, `g` and `G` remove trailing zeros from the fractional part.

In an argument for `b`, a `\c` stops the output: the built-in prints nothing more for the rest of the argument, the remaining arguments, and the rest of the format. The padding for the [field width](#field-width) is still printed. The `\0nnn` escape sequence is interpreted, but `\nnn` without the leading zero is printed as it is.

`%%` cannot have a position, flags, field width, or precision.

### Argument positions

An **argument position** is a positive decimal integer followed by `$`, as in `%2$s`. It selects the argument to consume by its index, so arguments can be consumed in any order and more than once. When the format is processed again, the indexes count from the argument after the last one consumed in the previous pass.

A format can mix conversion specifications with and without positions. A conversion specification without a position consumes the next argument in order, regardless of the positions of other conversion specifications. For example, `printf '%2$s %s %s\n' a b` prints `b a b`. In this case, the next pass starts after the argument with the highest position or the last argument consumed in order, whichever comes later. A `*` without a position for the field width or precision also consumes the next argument in order.

A conversion specification with a position must have an argument at that position.

### Flags

The flags are any number of the following characters in any order:

- `-` justifies the output to the left by appending spaces instead of prepending them.
- `+` prepends a plus sign to a non-negative number of a signed conversion (`d`, `i`, `f`, `F`, `e`, `E`, `g`, `G`).
- A space prepends a space to a non-negative number of a signed conversion, unless the `+` flag is also given.
- `#` selects the alternative form: `o` prints a leading zero, `x` and `X` prepend `0x` and `0X` to a non-zero value, and the floating-point conversions always print the decimal point. With `g` and `G`, it also keeps trailing zeros.
- `0` pads a number with zeros instead of spaces. It is ignored if the `-` flag is also given, if the number is infinity or NaN, or if an integer conversion has a precision.

The `#` flag cannot be used with `d`, `i`, `u`, `c`, `s`, `b`, and `q`. The `0` flag cannot be used with `c`, `s`, `b`, and `q`.

### Field width

A **field width** is a decimal integer that specifies the minimum number of bytes of the output of a conversion. If the output is shorter, the built-in pads it with spaces (or zeros, with the `0` flag). It does not truncate an output that is longer.

The field width may be `*`, in which case the built-in consumes an argument as an integer to use as the field width. A negative value means the `-` flag followed by the absolute value. With argument positions, write `*` followed by a position, as in `%2$*1$s`.

### Precision

A **precision** is a `.` followed by a decimal integer. A `.` with no integer after it means precision zero, which differs from having no precision at all.

- For the integer conversions, the precision is the minimum number of digits. The built-in prepends zeros to reach it. Without a precision, the minimum is one, so `printf '%d' 0` prints `0`. With precision zero, the value zero is printed as no digits, so `printf '%.0d' 0` and `printf '%.d' 0` print nothing.
- For `f`, `F`, `e`, and `E`, the precision is the number of digits after the decimal point. The default is six.
- For `g` and `G`, the precision is the maximum number of significant digits. The default is six, and zero means one.
- For `s`, `b`, and `q`, the precision is the maximum number of bytes printed. The built-in counts bytes, so it may cut a multibyte character in the middle. For `q`, the built-in quotes the argument before it applies the precision, so the result may not be valid shell input: `printf '%.2q' 'a b'` prints `'a`.

Like the field width, the precision may be `*` or `*` followed by a position. A negative value is treated as if no precision were given.

Neither a field width nor a precision may exceed 2147483647 in absolute value. A larger value in the format makes the format invalid. A larger value from an argument is an error, and the built-in uses 2147483647 instead, keeping the sign.

### Escape sequences

The built-in replaces each of the following escape sequences in the format with the corresponding character:

| Escape sequence | Replacement |
|-----------------|-------------|
| `\a`            | Alert (bell) |
| `\b`            | Backspace |
| `\e`            | Escape |
| `\f`            | Form feed |
| `\n`            | Newline |
| `\r`            | Carriage return |
| `\t`            | Horizontal tab |
| `\v`            | Vertical tab |
| `\\`            | Backslash |
| `\"`            | Double quote |
| `\'`            | Single quote |
| `\nnn`          | The byte whose value is the octal number *nnn* of one to three digits |
| `\xHH`          | The byte whose value is the hexadecimal number *HH* of one or two digits |
| `\uHHHH`        | The character whose Unicode code point is the hexadecimal number *HHHH* of one to four digits |
| `\UHHHHHHHH`    | The character whose Unicode code point is the hexadecimal number *HHHHHHHH* of one to eight digits |

If the value of `\nnn` exceeds 255 (octal 377), only its lowest eight bits are used. The built-in prints any other backslash as it is, but future versions may interpret more escape sequences.

Note that the format interprets `\nnn` without a leading zero, while the arguments for `b` require `\0nnn`.

### Numeric arguments

For an integer conversion, an argument is an integer in decimal, in octal with a leading `0`, or in hexadecimal with a leading `0x` or `0X`, optionally preceded by `+` or `-`. If the argument starts with a single or double quote, its value is the Unicode code point of the character following the quote. For example, `"'a"` is 97. An unsigned conversion of a negative value prints the value modulo 2<sup>64</sup>.

For a floating-point conversion, an argument is a decimal number such as `1.5` or `1.5e-3`, a hexadecimal number such as `0x1.8p3`, `inf`, `infinity`, or `nan`, optionally preceded by `+` or `-`. Floating-point numbers have the precision of the IEEE 754 binary64 format (double precision).

The built-in ignores spaces at the beginning of a numeric argument. If an argument is not completely a valid number or its value is out of range, the built-in reports an error and uses the part converted so far, or the largest or smallest representable value. For a floating-point conversion, a value too large in magnitude becomes infinity with the same sign. A nonzero value too small in magnitude to be represented precisely is also out of range, and the built-in uses the nearest representable value, which may be zero.

## Options

The **`-v`** (**`--variable`**) option takes a variable name as an argument and assigns the result to the [variable] instead of printing it. Because variables in yash-rs can currently hold only valid UTF-8 text, the built-in replaces any byte sequence in the result that is not valid UTF-8 with the replacement character U+FFFD. This behavior is subject to change when yash-rs supports other encodings or variables containing arbitrary bytes.

The **`--help`** option prints the [help](index.html#help-option) of the built-in.

## Operands

The first operand ***format*** is the format string described above.

The remaining operands ***argument***s are the values formatted according to the format.

## Errors

It is an error if:

- the format operand is missing,
- the format contains an incomplete or invalid conversion specification,
- the format contains a `\u` or `\U` escape sequence that is not a valid Unicode character,
- the variable name given to the `-v` option is empty or contains `=`,
- the variable given to the `-v` option is read-only, or
- the built-in cannot write to the standard output.

If the format is invalid, the built-in prints nothing.

It is also an error if a numeric argument is not a valid number or is out of range, or if a conversion specification with an argument position has no argument at that position. In these cases, the built-in still prints the whole result.

The [`portable` option](../environment/options.md#portable) causes additional errors; see [Compatibility](#compatibility).

## Exit status

Zero if successful; non-zero if an error occurs.

## Examples

```shell
$ printf 'Hello, %s!\n' world
Hello, world!
$ printf '%s=%d\n' a 1 b 2
a=1
b=2
$ printf '[%5s][%-5s][%.2s]\n' abc abc abc
[  abc][abc  ][ab]
$ printf '%05d %x %o %#X\n' 42 255 8 255
00042 ff 10 0XFF
$ printf '%.3f %e %g\n' 3.14159 1234.5 0.0001
3.142 1.234500e+03 0.0001
$ printf '%2$s %1$s\n' world hello
hello world
$ printf '%*d|\n' 5 42
   42|
$ printf '%d\n' "'A"
65
$ printf '%b\n' 'tab:\tend'
tab:	end
$ printf '%q\n' 'a b'
'a b'
$ printf -v greeting 'Hello, %s!' world
$ echo "$greeting"
Hello, world!
```

## Compatibility

The `printf` utility is specified by POSIX.1-2024. The following features are extensions that POSIX does not require:

- The `-v` option, which some other shells also support, and the long option names (`--variable`, `--help`)
- The floating-point conversion specifiers `f`, `F`, `e`, `E`, `g`, and `G`, which POSIX leaves optional
- The `q` conversion specifier, which POSIX mentions as a possible future addition
- `*` for the field width and precision
- The escape sequences `\e`, `\"`, `\'`, `\x`, `\u`, and `\U` in the format

POSIX leaves the behavior unspecified if a format mixes conversion specifications with and without argument positions. The behavior of the built-in in this case is the same as that of zsh, ksh93, and GNU coreutils, but yash 2 assigns the argument following the last consumed one to a conversion specification without a position. POSIX recommends treating a missing argument for a conversion specification with a position as an error, but some shells treat it as an empty string or zero.

POSIX requires the `c` conversion to print the first byte of the argument, and the precision of `s` and `b` to count bytes. This may print part of a multibyte character. Yash 2 and some other shells handle these in characters instead. POSIX leaves it unspecified whether `c` prints nothing or a null byte for an empty argument. The built-in prints nothing.

POSIX leaves the result unspecified for a backslash in the argument of `b` followed by a character other than those of the escape sequences `\\`, `\a`, `\b`, `\c`, `\f`, `\n`, `\r`, `\t`, `\v`, and `\0nnn`.

The flags `#` and `0` for the conversion specifiers listed above as not allowed are undefined in the C standard. Other shells may ignore them.

When the [`portable` option](../environment/options.md#portable) is set, the built-in rejects the extensions listed above and the following features, whose results POSIX does not specify:

- A format that mixes conversion specifications with and without argument positions
- A format containing a backslash that does not start one of the escape sequences POSIX specifies (`\\`, `\a`, `\b`, `\f`, `\n`, `\r`, `\t`, `\v`, and `\nnn`)
- An octal escape sequence whose value exceeds 255, in the format or in an argument of `b`
- A precision for `c`
- Arguments given to a format that contains no conversion specification consuming an argument
- An empty or missing argument for `c`
- An argument of `b` containing a backslash that does not start one of the escape sequences for `b` listed above
- An argument of `b` containing `\c` after as many bytes as the precision have been printed
- An integer argument preceded by blanks, or consisting only of a quote
- A negative argument for `u`, `o`, `x`, or `X`

The built-in prints nothing in these cases.

[variable]: ../language/parameters/variables.md
