# Aliases

**Alias substitution** replaces part of a command with a predefined string while parsing the command. Aliases are useful for creating shortcuts or customizing command behavior.

## Basic usage

Define an alias with the [`alias` built-in](../builtins/alias.md). When the first word in a [simple command](commands/simple.md) matches an alias, the shell replaces it with the alias definition before parsing the rest of the command.

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

Aliases can include multiple words, redirections, and delimiters. They can reference other aliases, which are expanded recursively.

```shell
$ alias dumb='> /dev/null'
$ dumb echo "Hello, World!"
```

Here, the second line becomes `> /dev/null echo "Hello, World!"`, so nothing is printed.

```shell
$ alias 2001='test "$(date +%Y)" = 2001 &&'
$ 2001 echo "Happy millennium!"
```

This expands to `test "$(date +%Y)" = 2001 && echo "Happy millennium!"`, printing the message if the year is 2001.

## Alias names

By POSIX.1-2024, alias names can use ASCII letters, digits, and `!`, `%`, `,`, `-`, `@`, `_`. Yash-rs allows any literal word as an alias name (no quotes or expansions). Alias names are case-sensitive.

## Recursion

Aliases can reference other aliases, creating a chain of substitutions. The shell expands aliases recursively until no more aliases are found. An alias is not substituted in the result of its own expansion, preventing infinite loops.

```shell,no_run
$ alias ll='ls -l'
$ alias l='ll -h'
$ l
total 40K
drwxr-xr-x 6 alice users 4.0K Jun 22 11:36 book
-rw-r--r-- 1 alice users  397 May 14 21:57 book.toml
-rwxr-xr-x 1 alice users 4.7K Jun 16 22:22 doctest.sh
-rw-r--r-- 1 alice users  20K May 28 00:04 LICENSE
drwxr-xr-x 3 alice users 4.0K May 31 02:11 src
```

```shell,no_run
$ alias ls='ls -F'
$ ls
book/  book.toml  doctest.sh*  LICENSE  src/
```

## Continued substitution

If an alias definition ends with a blank, the next word is also checked for alias substitution, even if it is not the first word of the command. This is useful for utilities that take another command as an argument.

```shell,no_run
$ alias greet='echo Hello,'
$ alias time='time -p '
$ time greet World
Hello, World
real 0.00
user 0.00
sys 0.00
```

If the `time` alias does not end with a blank, the next word is not substituted:

```shell,no_run
$ alias greet='echo Hello,'
$ alias time='time -p'
$ time greet World
time: cannot run greet: No such file or directory
real 0.01
user 0.00
sys 0.00
```

Note: In yash and many other shells, this behavior only applies if the next word is a whole word, not a part of a word. In the following example, `a` follows a blank resulting from alias substitution for `q`, but it is inside quotes, so it is not substituted:

```shell
$ alias echo='echo ' q="'[ " a=b
$ echo q a ]'
[  a ]
```

<!-- TODO: Global aliases not yet implemented
## Global aliases

If an alias is defined with the `-g` option, it is a **global alias**. Global aliases are substituted in any word of a command, not just the first word. This is useful as a shorthand for frequently used pipelines or other command sequences.
-->

<!-- ```shell -->
<!--
$ alias -g NE='| grep -v "^$"' # filters empty lines out
$ printf "Hello\n\nWorld\n" NE
Hello
World
-->
<!-- ``` -->

## Miscellaneous

To prevent alias substitution for a word, [quote](words/quoting.md) it.

Aliases become effective after the defining command is executed. Since commands are parsed and executed line by line, aliases defined in the current line are not available in the same line.

Remove an alias with the `unalias` built-in.

## Aliases and functions

[Functions](functions.md) are similar to aliases in that both let you define names for command sequences. Functions are better for complex logic—they can [take parameters](functions.md#function-parameters), use [local variables](parameters/variables.md#local-variables), and include conditionals, loops, and other [compound commands](commands/index.html#commands-1). Aliases are better for syntactic manipulation, such as inserting a [pipeline](commands/pipelines.md) or redirection, because they are expanded as the command is parsed.
