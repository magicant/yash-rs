# Aliases

**Alias substitution** is a feature that replaces part of a command with a predefined string while parsing the command. It is useful for creating shortcuts or modifying command behavior.

## Basic usage

An alias is defined using the `alias` built-in. When a simple command starts with an alias name, the shell replaces the alias with its definition before continuing to parse the command.

```shell,no_run
$ alias ll='ls -l'
$ ll
total 40
drwxr-xr-x 6 alice users  4096 Jun 21 12:57 book
-rw-r--r-- 1 alice users   397 May 14 21:57 book.toml
-rwxr-xr-x 1 alice users  4801 Jun 16 22:22 doctest.sh
-rw-r--r-- 1 alice users 20138 May 28 00:04 LICENSE
drwxr-xr-x 3 alice users  4096 May 31 02:11 src
```

Since aliases substitute source code, they can include any valid command syntax. They can contain multiple words, redirections, and delimiters. They can even include other aliases, which are expanded recursively.

```shell
$ alias dumb='> /dev/null'
$ dumb echo "Hello, World!"
```

In this example, the second line becomes `> /dev/null echo "Hello, World!"` after alias substitution, which prints nothing because of the redirection.

```shell
$ alias 2001='test "$(date +%Y)" = 2001 &&'
$ 2001 echo "Happy millennium!"
```

In this example, the second line becomes `test "$(date +%Y)" = 2001 && echo "Happy millennium!"` after alias substitution, which prints "Happy millennium!" if the current year is 2001.

## Alias names

In POSIX.1-2024, alias names must consist of ASCII letters, digits, and the following symbols: `!`, `%`, `,`, `-`, `@`, `_`. Yash-rs extends this to allow any literal word as an alias name. Literal words are words that do not contain any [quotes](words/quoting.md) or [expansions](words/index.html#word-expansion). Alias names are case-sensitive.

## Recursion

## Continued substitution

## Global aliases

## Miscellaneous

Aliases become effective after the command that defines them is executed. Since commands are parsed and executed line by line, aliases defined in the current line are not available in the same line.

To remove an alias, use the `unalias` built-in.

## Related topics
