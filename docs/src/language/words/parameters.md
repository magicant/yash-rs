# Parameter expansion

**Parameter expansion** retrieves the value of a [parameter](../parameters/index.html) when a command is executed. The basic syntax is `${parameter}`.

```shell
$ user="Alice" # define a variable
$ echo "Hello, ${user}!" # expand the variable
Hello, Alice!
```

## Unset parameters

If a parameter is unset, the shell expands it to an empty string by default.

```shell
$ unset user
$ echo "Hello, ${user}!"
Hello, !
```

If the [`nounset` shell option](../../environment/options.md#option-list) is enabled, expanding an unset parameter is an error:

```shell
$ set -o nounset
$ echo "Hello, ${user}!"
error: cannot expand unset parameter
 --> <stdin>:2:14
  |
2 | echo "Hello, ${user}!"
  |              ^^^^^^^ parameter `user` is not set
  |
  = info: unset parameters are disallowed by the nounset option
```

Using `nounset` is recommended to catch typos in variable names.

## Omitting braces

Braces are optional if the parameter is:

- a [variable](../parameters/variables.md) name with only ASCII letters, digits, and underscores (e.g., `$HOME`, `$user`)
- a [special parameter] (e.g., `$?`, `$#`)
- a single-digit [positional parameter](../parameters/positional.md) (e.g., `$1`, `$2`)

For variable names, the shell uses the longest possible name after `$`, regardless of whether the variable exists:

```shell
$ user="Alice"
$ unset username
$ echo "Hello, $username!" # $user is not considered
Hello, !
```

For positional parameters, only a single digit is used, even if followed by more digits:

```shell
$ set foo bar baz # set three positional parameters
$ echo "$12" # $1 expands to the first positional parameter
foo2
```

## Modifiers

**Modifiers** change the value of a parameter during expansion. Modifiers can only be used in braced expansions, and only one modifier is allowed per expansion.

### Length

The **length** modifier `${#parameter}` returns the number of characters in the parameter's value.

```shell
$ user="Alice"
$ echo "Length of user: ${#user}"
Length of user: 5
```

As an extension, the length modifier can be used with arrays or [special parameters](../parameters/special.md) `*` or `@`, applying the modifier to each element:

```shell
$ users=(Alice Bob Charlie)
$ echo "Lengths of users: ${#users}"
Lengths of users: 5 3 7
$ set yellow red green blue # set four positional parameters
$ echo "Lengths of positional parameters: ${#*}"
Lengths of positional parameters: 6 3 5 4
```

### Switch

The **switch** modifier changes the result based on whether a parameter is set or empty. There are eight forms:

- `${parameter-word}` – Use `word` if `parameter` is unset.
- `${parameter:-word}` – Use `word` if `parameter` is unset or empty.
- `${parameter+word}` – Use `word` if `parameter` is set.
- `${parameter:+word}` – Use `word` if `parameter` is set and not empty.
- `${parameter=word}` – Assign `word` to `parameter` if unset, using the new value.
- `${parameter:=word}` – Assign `word` to `parameter` if unset or empty, using the new value.
- `${parameter?word}` – Error with `word` if `parameter` is unset.
- `${parameter:?word}` – Error with `word` if `parameter` is unset or empty.

Examples:

```shell
$ user="Alice"
$ echo "Hello, ${user-World}!"
Hello, Alice!
$ unset user
$ echo "Hello, ${user-World}!"
Hello, World!
```

```shell
$ unset PATH
$ PATH="/bin${PATH:+:$PATH}"
$ echo "$PATH"
/bin
$ PATH="/usr/bin${PATH:+:$PATH}"
$ echo "$PATH"
/usr/bin:/bin
```

```shell
$ unset user
$ echo "Hello, ${user=Alice}!"
Hello, Alice!
$ echo "Hello, ${user=Bob}!"
Hello, Alice!
```

```shell
$ user="Alice"
$ echo "Hello, ${user?tell me your name}!"
Hello, Alice!
$ unset user
$ echo "Hello, ${user?tell me your name}!"
error: tell me your name
 --> <stdin>:4:14
  |
4 | echo "Hello, ${user?tell me your name}!"
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^ parameter `user` is not set
```

In all cases, the following expansions are performed on `word` before use:

- [Tilde expansion](../words/tilde.md) (unless the parameter expansion is in [double quotes](quoting.md#double-quotes))
- Parameter expansion (recursive!)
- [Command substitution](command_substitution.md)
- [Arithmetic expansion](arithmetic.md)

For the `=` and `:=` forms, [quote removal](quoting.md#quote-removal) is also performed before assignment. Assignment only works for [variables](../parameters/variables.md), not [special](../parameters/special.md) or [positional parameters](../parameters/positional.md).

If `word` is empty in the `?` and `:?` forms, a default error message is used.

The [`nounset` option](../../environment/options.md#option-list) does not apply to expansions with a switch modifier.

### Trim

The **trim** modifier removes leading or trailing characters matching a pattern from a parameter's value. There are four forms:

- `${parameter#pattern}` – Remove the shortest match of `pattern` from the start.
- `${parameter##pattern}` – Remove the longest match of `pattern` from the start.
- `${parameter%pattern}` – Remove the shortest match of `pattern` from the end.
- `${parameter%%pattern}` – Remove the longest match of `pattern` from the end.

The value is matched against the [pattern](../../patterns.md), and the matching part is removed.

```shell
$ var="banana"
$ echo "${var#*a}"
nana
$ echo "${var##*a}"

$ echo "${var%a*}"
banan
$ echo "${var%%a*}"
b
```

The pattern is expanded before use:

- [Tilde expansion](../words/tilde.md)
- Parameter expansion (recursive!)
- [Command substitution](command_substitution.md)
- [Arithmetic expansion](arithmetic.md)

You can quote part or all of the pattern to treat it literally:

```shell
$ asterisks="***"
$ echo "${asterisks##*}" # removes the whole value

$ echo "${asterisks##\*}" # removes the first *
**
$ echo "${asterisks##'**'}" # removes the first two *
*
```

### Compatibility

Some modifiers are ambiguous when used with a certain [special parameter]. Yash and many other shells interpret `${##}`, `${#-}`, and `${#?}` as length modifiers applied to special parameters `#`, `-`, and `?`, not as switch or trim modifiers applied to `#`. The POSIX standard is unclear on this point.

The result is unspecified in POSIX for:

- a length or switch modifier applied to special parameter `*` or `@`
- a trim modifier applied to special parameter `#`, `*`, or `@`

[special parameter]: ../parameters/special.md
