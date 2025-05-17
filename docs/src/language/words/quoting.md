# Quoting and escaping

Quoting and escaping control how the shell interprets special characters and whitespace in commands and arguments. This section describes the quoting and escaping mechanisms available in the shell.

## Single quotes

Single quotes enclose a string and prevent the shell from interpreting special characters. Everything inside single quotes is treated literally, including spaces and special characters.

For example, the following command prints the string `"$foo"` without interpreting the `$` as a parameter expansion or the `"` as a double quote:

```shell
$ echo '"$foo"'
"$foo"
```

Single quotes can contain newline characters:

```shell
$ echo 'foo
> bar'
foo
bar
```

You cannot include a single quote character inside a single-quoted string. You need to use double quotes or a backslash to escape it.

## Double quotes

Double quotes enclose a string. Most characters inside double quotes are treated literally, but some characters are still interpreted by the shell:

- `$`: Parameter expansion, command substitution, and arithmetic expansion
- `` ` ``: Command substitution
- `\`: Character escape, only before `$`, `` ` ``, `\`, and newline

For example, single quote characters are treated literally and parameter expansion is performed inside double quotes:

```shell
$ foo="*  *"
$ echo "foo='$foo'"
foo='*  *'
```

Double quotes prevent field splitting and pathname expansion on the result of expansions. If the argument to the echo utility were not double-quoted in the above example, the output might have been different depending on the result of field splitting and pathname expansion.

## Backslash

The backslash escapes special characters, allowing you to include them in a string without interpretation.

Outside double quotes, a backslash can escape any character except newline. For example:

```sh
cat My\ Diary.txt
```

This prints the contents of the file `My Diary.txt`.

When used in double quotes, the backslash only escapes the following characters: `$`, `` ` ``, and `\`. For example:

```sh
cat "My\ Diary\$.txt"
```

This will print the contents of the file `My\ Diary$.txt`. Note that the backslash before the space is treated literally, and the backslash before the dollar sign is treated as an escape character.

### Line continuation

When a backslash is placed at the end of a line, it indicates that the command continues on the next line. This is useful for breaking long commands into multiple lines for better readability. The combination of a backslash and newline is ignored by the shell as if it were not there. Line continuation can be used inside and outside double quotes.

```shell
$ echo "This is a long command that \
> continues on the next line"
This is a long command that continues on the next line
```

To treat a newline literally rather than as a line continuation, use single or double quotes.

## Dollar single quotes

Dollar single quotes (`$'...'`) are used to specify strings with escape sequences, similar to those in C. The content inside the quotes is parsed, and recognized escape sequences are replaced with their corresponding characters.

For example, `\n` is replaced with a newline character:

```shell
$ echo $'foo\nbar'
foo
bar
```

The following escape sequences are recognized inside dollar single quotes:

- `\\` – backslash
- `\'` – single quote
- `\"` – double quote
- `\n` – newline
- `\t` – tab
- `\r` – carriage return
- `\a` – alert (bell)
- `\b` – backspace
- `\e` or `\E` – escape
- `\f` – form feed
- `\v` – vertical tab
- `\?` – question mark
- `\cX` – control character (e.g., `\cA` for `^A`)
- `\xHH` – byte with hexadecimal value `HH` (1–2 hex digits)
- `\uHHHH` – Unicode character with hexadecimal value `HHHH` (4 hex digits)
- `\UHHHHHHHH` – Unicode character with hexadecimal value `HHHHHHHH` (8 hex digits)
- `\NNN` – byte with octal value `NNN` (1–3 octal digits)

Unrecognized or incomplete escape sequences cause an error.

A backslash followed by a newline is not treated as a line continuation inside dollar single quotes; they are rejected as an error.

Example with Unicode:

```shell
$ echo $'\u3042'
あ
```

Dollar single quotes are useful for specifying strings with special characters<!-- or binary data-->.

<p class="warning">
In the current implementation, escape sequences that produce a byte are treated as a Unicode character with the same value and converted to UTF-8. This means that byte values greater than or equal to 0x80 are converted to two bytes of UTF-8. This behavior does not conform to the POSIX standard and may change in the future.
</p>
