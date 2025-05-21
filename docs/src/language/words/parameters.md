# Parameter expansion

**Parameter expansion** retrieves the value of a [parameter](../parameters/README.md) when the containing command is executed.
The basic syntax is `${parameter}`.

```shell
$ user="Alice" # define a variable
$ echo "Hello, ${user}!" # expand the variable
Hello, Alice!
```

## Unset parameters

If a parameter is unset, the shell by default expands it to an empty string.

```shell
$ unset user # unset the variable
$ echo "Hello, ${user}!"
Hello, !
```

However, if the `nounset` shell option is set, the shell treats unset parameters as an error.

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

It is highly recommended to use this option to catch misspelled variable names in scripts.

## Omitting braces

The braces are optional if the parameter is one of:

- a variable name that consists only of ASCII letters, digits, and underscores (e.g., `$HOME`, `$user`, etc.)
- a special parameter (e.g., `$?`, `$#`, etc.)
- a single-digit positional parameter (e.g., `$1`, `$2`, etc.)

For a variable name without braces, the shell assumes the longest possible name regardless of whether the named variable exists.
In the following example, `username` is the longest name after `$`, so the shell attempts to expand it. The existing variable `user` is not considered.

```shell
$ user="Alice"
$ unset username
$ echo "Hello, $username!"
Hello, !
```

For a positional parameter without braces, the shell assumes a single-digit parameter, even if it is followed by another digit. In the following example, `1` is treated as a positional parameter, while `2` is treated as a literal character.

```shell
$ set foo bar baz # set three positional parameters
$ echo "$12"
foo2
```

## Modifiers

**Modifiers** manipulate the value of a parameter during expansion. Modifiers can only be used in braced parameter expansions. At most one modifier can be used in a single expansion.

### Length

The **length** modifier `${#parameter}` returns the length of the value of the parameter. The length is the number of characters in the value, not the number of bytes.

```shell
$ user="Alice"
$ echo "Length of user: ${#user}"
Length of user: 5
```

As an extension to POSIX, the length modifier can also be used with an array or special parameter `*` or `@`, in which case the modifier is applied to each element of the array or positional parameters.

```shell
$ users=(Alice Bob Charlie)
$ echo "Lengths of users: ${#users}"
Lengths of users: 5 3 7
$ set yellow red green blue # set four positional parameters
$ echo "Lengths of positional parameters: ${#*}"
Lengths of positional parameters: 6 3 5 4
```

### Switch

The **switch** modifier triggers a specific action based on (non-)existence of a parameter. There are eight forms of the switch modifier:

- `${parameter-word}` – If `parameter` is unset, use `word` as the value.
- `${parameter:-word}` – If `parameter` is unset or empty, use `word` as the value.
- `${parameter+word}` – If `parameter` is set, use `word` as the value.
- `${parameter:+word}` – If `parameter` is set and not empty, use `word` as the value.
- `${parameter=word}` – If `parameter` is unset, assign `word` to it and use `word` as the value.
- `${parameter:=word}` – If `parameter` is unset or empty, assign `word` to it and use `word` as the value.
- `${parameter?word}` – If `parameter` is unset, fail with `word` as the error message.
- `${parameter:?word}` – If `parameter` is unset or empty, fail with `word` as the error message.

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
 --> <stdin>:1:14
  |
1 | echo "Hello, ${user?tell me your name}!"
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^ parameter `user` is not set
```

In all cases, `word` is expanded before being used; specifically, the following expansions are performed:

- [Tilde expansion](../words/tilde.md)
- Parameter expansion (recursive!)
- Command substitution
- Arithmetic expansion

For the `=` and `:=` forms, quote removal is also performed on `word` before assignment.

An empty `word` is allowed. A default error message is used if `word` is empty for the `?` and `:?` forms.

For the `=` and `:=` forms, the assignment is possible only if the parameter is a variable. If the parameter is a special or positional parameter, the expansion fails with an error message.

The `nounset` shell option does not apply to parameters expanded with a switch modifier.

### Trim

The **trim** modifier removes leading or trailing characters from the value of a parameter. There are four forms of the trim modifier:

- `${parameter#pattern}` – Remove the shortest match of `pattern` from the start of the value.
- `${parameter##pattern}` – Remove the longest match of `pattern` from the start of the value.
- `${parameter%pattern}` – Remove the shortest match of `pattern` from the end of the value.
- `${parameter%%pattern}` – Remove the longest match of `pattern` from the end of the value.

In all cases, the value is matched against the pattern. <!-- TODO: See [Pattern matching]() for more details. -->
The part of the value that matches the pattern is removed.

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

The pattern is expanded before being used, specifically, the following expansions are performed:

- [Tilde expansion](../words/tilde.md)
- Parameter expansion (recursive!)
- Command substitution
- Arithmetic expansion

You can quote (part of) the pattern to treat it literally:

```shell
$ asterisks="***"
$ echo "${asterisks#*}" # matches nothing
***
$ echo "${asterisks#\*}" # removes the first * only
**
$ echo "${asterisks#'**'}" # removes the first two *s
*
```

### Compatibility

Some modifiers are ambiguous when used with a certain special parameter. Yash and many other shells interpret `${##}`, `${#-}`, and `${#?}` as length modifiers applied to special parameters `#`, `-`, and `?`, respectively, rather than switch and trim modifiers being applied to special parameter `#`. The POSIX standard is unclear on this point.

The result is unspecified in POSIX for the following cases:

- a length or switch modifier applied to special parameter `*` or `@`
- a trim modifier applied to special parameter `#`, `*`, or `@`
