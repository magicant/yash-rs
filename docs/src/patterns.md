# Pattern matching

This section describes the pattern matching notation used in the shell. Patterns are used in [pathname expansion](language/words/globbing.md), `case` commands, and [parameter expansion modifiers](language/words/parameters.md#modifiers).

## Literals

A **literal** is a character that matches itself. For example, the pattern `a` matches the character `a`. All characters are literals except for the special characters described below.

## Quoting

Quoting makes a special character behave as a literal. See the [Quoting](language/words/quoting.md) section for details. Additionally, for unquoted parts of a pattern produced by [parameter expansion](language/words/parameters.md), [command substitution](language/words/command_substitution.md), or [arithmetic expansion](language/words/arithmetic.md), backslashes escape the following character, but such backslashes are not subject to quote removal.

In this example, no pathname expansion occurs because the special characters are quoted:

```shell
$ echo a\*b
a*b
$ asterisk='*'
$ echo "$asterisk"
*
$ quoted='a\*b'
$ echo $quoted
a\*b
```

## Special characters

The following characters have special meanings in patterns:

- `?` – Matches any single character.
- `*` – Matches any number of characters, including none.
- `[...]` – Matches any single character from the set of characters inside the brackets. For example, `[abc]` matches `a`, `b`, or `c`. Ranges can be specified with a hyphen, like `[a-z]` for lowercase letters.
- `[!...]` and `[^...]` – Matches any single character not in the set of characters inside the brackets. For example, `[!abc]` matches any character except `a`, `b`, or `c`.

The `[^...]` form is not supported in all shells; prefer using `[!...]` for compatibility.

```shell,no_run
$ echo ?????? # prints all six-character long filenames
Videos
$ echo Do* # prints all files starting with Do
Documents Downloads
$ echo [MP]* # prints all files starting with M or P
Music Pictures
$ echo *[0-9] # prints all files ending with a digit
foo.bak.1 foo.bak.2 bar.bak.3
```

### Special elements in brackets

Bracket expressions `[…]` can include special elements:

- **Character classes**: `[:class:]` matches any character in the specified class. Available classes:
    - `[:alnum:]` – Alphanumeric characters (letters and digits)
    - `[:alpha:]` – Alphabetic characters (letters)
    - `[:blank:]` – Space and tab characters
    - `[:cntrl:]` – Control characters
    - `[:digit:]` – Digits (0-9)
    - `[:graph:]` – Printable characters except space
    - `[:lower:]` – Lowercase letters
    - `[:print:]` – Printable characters including space
    - `[:punct:]` – Punctuation characters
    - `[:space:]` – Space characters (space, tab, newline, etc.)
    - `[:upper:]` – Uppercase letters
    - `[:xdigit:]` – Hexadecimal digits (0-9, a-f, A-F)

    ```shell,no_run
    $ echo [[:upper:]]* # prints all files starting with an uppercase letter
    Documents Downloads Music Pictures Videos
    $ echo *[[:digit:]~] # prints all files ending with a digit or tilde
    foo.bak.1 foo.bak.2 bar.bak.3 baz~
    ```

- **Collating elements**: `[.char.]` matches the collating element `char`. A collating element is a character or sequence of characters treated as a single unit in pattern matching. Collating elements depend on the current locale and are not yet implemented in yash-rs.

- **Equivalence classes**: `[=char=]` matches the equivalence class of `char`. An equivalence class is a set of characters considered equivalent for matching purposes (e.g., `a` and `A` in some locales). This feature is not yet implemented in yash-rs.

<p class="warning">
Locale support is not yet implemented in yash-rs. Currently, all patterns match the same characters regardless of locale. Collating elements and equivalence classes simply match the characters as they are, without any special treatment.
</p>

<!-- TODO caseglob -->

## Special considerations for pathname expansion

See the [Pathname expansion](language/words/globbing.md#pattern-syntax) section for additional rules that apply in pathname expansion.
