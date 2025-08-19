# Arithmetic expansion

[arithmetic expression]: ../../arithmetic.md

**Arithmetic expansion** evaluates an [arithmetic expression] and replaces it with the result. The syntax is `$((expression))`.

```shell
$ echo $((1 + 2))
3
$ echo $((2 * 3 + 4))
10
$ echo $((2 * (3 + 4)))
14
```

Arithmetic expansion works in two steps. First, the expression is processed for [parameter expansion](parameters.md), nested arithmetic expansion, [command substitution](command_substitution.md), and [quote removal](quoting.md#quote-removal). Then, the resulting string is parsed as an [arithmetic expression], and the result replaces the expansion.

```shell
$ x=2
$ echo $(($x + $((3 * 4)) + $(echo 5)))
19
```

## Variables

The value of variables appearing as [parameter expansions](parameters.md) does not have to be numeric, but the resulting [arithmetic expression] must be valid.

```shell
$ seven=7
$ var='6 * sev'
$ echo $((${var}en)) # expands to $((6 * seven))
42
```

```shell
$ seven='3 + 4'
$ echo $((2 * $seven)) # expands to $((2 * 3 + 4)), mind the precedence
10
$ echo $((2 * seven))
error: error evaluating the arithmetic expansion
 --> <arithmetic_expansion>:1:5
  |
1 | 2 * seven
  |     ^^^^^ invalid variable value: "3 + 4"
  |
 ::: <stdin>:3:6
  |
3 | echo $((2 * seven))
  |      -------------- info: arithmetic expansion appeared here
  |
```

## Quoting

[Backslash escaping](quoting.md#backslash) is the only supported quoting mechanism in arithmetic expansion. It can escape `$`, `` ` ``, and `\`. However, escaped characters would never produce a valid [arithmetic expression] after [quote removal](quoting.md#quote-removal), so they are not useful in practice.

```shell
$ echo $((\$x))
error: error evaluating the arithmetic expansion
 --> <arithmetic_expansion>:1:1
  |
1 | $x
  | ^ invalid character
  |
 ::: <stdin>:1:6
  |
1 | echo $((\$x))
  |      -------- info: arithmetic expansion appeared here
  |
```
