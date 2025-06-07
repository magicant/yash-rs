# Grouping

A **grouping** command ties multiple commands together, making them treated as a single command. This is useful for running multiple commands in a [pipeline](pipelines.md) or and-or list.

## Braces

Commands grouped in braces `{ ... }` are executed in the current shell.

```shell
$ { echo "Hello"; echo "World"; }
Hello
World
```

A group may span multiple lines:

```shell
$ {
> echo "Hello"
> echo "World"
> }
Hello
World
```

Since `{` and `}` are reserved words, they must appear as separate words. See [examples in the Keywords section](../words/keywords.md#examples).

## Subshells

Commands grouped in parentheses `( ... )` are executed in a subshell, that is, a copy of the current shell environment. Changes made in a subshell do not affect the parent shell.

```shell
$ greeting="Morning"
$ (greeting="Hello"; echo "$greeting")
Hello
$ echo "$greeting"
Morning
```

Since `(` and `)` are operators, they can be used without spaces as shown above.

## Compatibility

Many shells assign special meaning to two adjacent `(` characters. For maximum compatibility, open parentheses should be separated by a space to be recognized as nested subshells:

```shell
$ ( (echo "Hello"))
Hello
```
