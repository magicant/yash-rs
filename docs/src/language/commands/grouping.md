# Grouping

A **grouping** command combines multiple commands so they are treated as a single command. This is useful for running several commands together in a [pipeline](pipelines.md) or an [and-or list](exit_status.md#and-or-lists).

## Braces

Commands grouped in braces `{ … }` run in the current [shell environment].

```shell
$ { echo "Hello"; echo "World"; }
Hello
World
```

A group can span multiple lines:

```shell
$ {
> echo "Hello"
> echo "World"
> }
Hello
World
```

Since `{` and `}` are [reserved words](../words/keywords.md), they must appear as separate words. See [examples in the Keywords section](../words/keywords.md#examples).

Braces are especially useful for treating several commands as a single unit in [pipelines](pipelines.md) or [and-or lists](exit_status.md#and-or-lists):

```shell,hidelines=#
$ { echo "Hello"; echo "World"; } | grep "Hello"
Hello
```

```shell,hidelines=#
#$ HOME=$PWD
$ test -f ~/cache/file || { mkdir -p ~/cache; > ~/cache/file; }
```

## Subshells

Commands grouped in parentheses `( … )` run in a subshell—a copy of the current [shell environment]. Changes made in a subshell do not affect the parent shell.

```shell
$ greeting="Morning"
$ (greeting="Hello"; echo "$greeting")
Hello
$ echo "$greeting"
Morning
```

Since `(` and `)` are operators, they can be used without spaces.

## Compatibility

Some shells treat two adjacent `(` characters specially. For best compatibility, separate open parentheses with a space to nest subshells:

```shell
$ ( (echo "Hello"))
Hello
```

[shell environment]: ../../environment/index.html
