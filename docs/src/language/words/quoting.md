# Quoting and escaping

Quoting and escaping control how the shell interprets special characters and whitespace in commands and arguments. This section describes the quoting and escaping mechanisms available in the shell.

## Single quotes

Single quotes enclose a string and prevent the shell from interpreting special characters. Everything inside single quotes is treated literally, including spaces and special characters.

For example, the following command prints the string `$"foo"` without interpreting the `$` as a parameter expansion or the `"` as a double quote:

```shell
echo '$"foo"'
```

Single quotes can contain newline characters:

```shell
echo 'foo
bar'
```

This prints:

```text
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
foo="*  *"
echo "foo='$foo'"
```

This prints:

```text
foo='*  *'
```

Double quotes prevent field splitting and pathname expansion on the result of expansions. If the argument to the echo utility were not double-quoted in the above example, the output might have been different depending on the result of field splitting and pathname expansion.

## Backslash

The backslash escapes special characters, allowing you to include them in a string without interpretation.

Outside double quotes, a backslash can escape any character except newline. For example:

```shell
cat My\ Diary.txt
```

This prints the contents of the file `My Diary.txt`.

When used in double quotes, the backslash only escapes the following characters: `$`, `` ` ``, and `\`. For example:

```shell
cat "My\ Diary\$.txt"
```

This will print the contents of the file `My\ Diary$.txt`. Note that the backslash before the space is treated literally, and the backslash before the dollar sign is treated as an escape character.

### Line continuation

When a backslash is placed at the end of a line, it indicates that the command continues on the next line. This is useful for breaking long commands into multiple lines for better readability. The combination of a backslash and newline is ignored by the shell as if it were not there. Line continuation can be used inside and outside double quotes.

```shell
echo "This is a long command that \
continues on the next line"
```

This will print:

```text
This is a long command that continues on the next line
```

To treat a newline literally rather than as a line continuation, use single or double quotes.

## Dollar single quotes

TODO: to be documented
