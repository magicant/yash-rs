# Arithmetic expressions

**Arithmetic expressions** in [arithmetic expansion](language/words/arithmetic.md) are similar to C expressions. They can include numbers, variables, operators, and parentheses.

## Numeric constants

Numeric constants can be:

- Decimal, written as-is (e.g., `42`)
- Octal, starting with `0` (e.g., `042`)
- Hexadecimal, starting with `0x` or `0X` (e.g., `0x2A`)

```shell
$ echo $((42))   # decimal
42
$ echo $((042))  # octal
34
$ echo $((0x2A)) # hexadecimal
42
```

All integers are signed 64-bit values, ranging from `-9223372036854775808` to `9223372036854775807`.

C-style integer suffixes (`U`, `L`, `LL`, etc.) are not supported.

Floating-point constants are not supported, but may be added in the future.

## Variables

[Variables](language/parameters/variables.md) in arithmetic expressions appear as bare names. Variable names can include Unicode letters, Unicode digits, and ASCII underscores (`_`), but cannot start with a digit. Variables must have numeric values.

```shell
$ a=5 b=10
$ echo $((a + b))
15
```

If a variable is unset and the `nounset` [shell option](environment/options.md) is off, it is treated as zero:

```shell
$ unset x; set +o nounset
$ echo $((x + 3))
3
```

If the `nounset` option is on, an error occurs when trying to use an unset variable:

```shell
$ unset x; set -o nounset
$ echo $((x + 3))
error: cannot expand unset parameter
 --> <arithmetic_expansion>:1:1
  |
1 | x + 3
  | ^ parameter `x` is not set
  |
 ::: <stdin>:2:6
  |
2 | echo $((x + 3))
  |      ---------- arithmetic expansion appeared here
  |
  = note: unset parameters are disallowed by the nounset option
```

If a variable has a non-numeric value, an error occurs.

```shell
$ x=foo
$ echo $((x + 3))
error: error evaluating the arithmetic expansion
 --> <arithmetic_expansion>:1:1
  |
1 | x + 3
  | ^ invalid variable value: "foo"
  |
 ::: <stdin>:2:6
  |
2 | echo $((x + 3))
  |      ---------- arithmetic expansion appeared here
```

Currently, variables in arithmetic expressions must have a single numeric value. In the future, more complex values may be supported.

## Operators

The following operators are supported, in order of precedence:

1. `(` `)` – grouping
2. Postfix:
    - `++` – increment
    - `--` – decrement
3. Prefix:
    - `+` – no-op
    - `-` – numeric negation
    - `~` – bitwise negation
    - `!` – logical negation
    - `++` – increment
    - `--` – decrement
4. Binary (left associative):
    - `*` – multiplication
    - `/` – division
    - `%` – modulus
5. Binary (left associative):
    - `+` – addition
    - `-` – subtraction
6. Binary (left associative):
    - `<<` – left shift
    - `>>` – right shift
7. Binary (left associative):
    - `<` – less than
    - `<=` – less than or equal to
    - `>` – greater than
    - `>=` – greater than or equal to
8. Binary:
    - `==` – equal to
    - `!=` – not equal to
9. Binary:
    - `&` – bitwise and
10. Binary:
    - `|` – bitwise or
11. Binary:
    - `^` – bitwise xor
12. Binary:
    - `&&` – logical and
13. Binary:
    - `||` – logical or
14. Ternary (right associative):
    - `?` `:` – conditional expression
15. Binary (right associative):
    - `=` – assignment
    - `+=` – addition assignment
    - `-=` – subtraction assignment
    - `*=` – multiplication assignment
    - `/=` – division assignment
    - `%=` – modulus assignment
    - `<<=` – left shift assignment
    - `>>=` – right shift assignment
    - `&=` – bitwise and assignment
    - `|=` – bitwise or assignment
    - `^=` – bitwise xor assignment

Other operators, such as `sizeof`, are not supported.

## Compatibility

POSIX.1-2024 defines arithmetic expressions on the basis of C.

POSIX requires support for `signed long` integers. This implementation uses signed 64-bit integers, which is at least as wide as `long` on all common platforms. Future versions may support wider integers. Other implementations may only support narrower integers.

POSIX does not require support for the `++` and `--` operators. Dash 0.5.12 treats the `++` prefix operator as two `+` operators, effectively making it a no-op.
