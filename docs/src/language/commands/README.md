# Commands

This section summarizes the syntax of commands in the shell language. **Commands** (in the broad sense) are instructions to the shell to perform actions such as running programs, changing the environment, or controlling execution flow. For details, see the linked sections below.

## Whole scripts

A shell script consists of a sequence of lists separated by newlines. The shell reads and parses input line by line until it forms a complete list, executes that list, then continues to the next.

```shell
$ echo "Hello, World!"
Hello, World!
$ for fruit in apple banana cherry; do
>     echo "I like $fruit"
> done
I like apple
I like banana
I like cherry
```

## Lists

A [list](lists.md) is a sequence of and-or lists separated by `;` or `&`. Lists let you write multiple commands on one line or run commands asynchronously.

```shell
$ echo "Hello"; echo "World"
Hello
World
```

## And-or lists

An [and-or list](exit_status.md#and-or-lists) is a sequence of pipelines separated by `&&` or `||`. This lets you control execution flow based on the success or failure of previous commands.

```shell
$ test -f /nonexistent/file && echo "File exists" || echo "File does not exist"
File does not exist
```

## Pipelines

A [pipeline](pipelines.md) is a sequence of commands connected by `|`, where the output of one command is passed as input to the next. Pipelines let you combine commands to process data in a stream.

You can prefix a pipeline with the `!` [reserved word](../words/keywords.md) to negate its [exit status](exit_status.md):

```shell,no_run
$ ! tail file.txt | grep TODO
TODO: Fix this issue
```

## Commands

A command (in the narrow sense) is a pipeline component: a simple command, a compound command, or a function definition.

A [simple command](simple.md) runs a utility or function, or assigns values to [variables](../parameters/variables.md).

**Compound commands** control execution flow and include:

- [Grouping commands](grouping.md): Group multiple commands to run as a unit, in the current shell or a subshell.
- [If commands](exit_status.md#if-commands): Run commands conditionally based on exit status.
- [Case commands](case.md): Run commands based on pattern matching a value.
- [For loops](loops.md#for-loops): Iterate over a list, running commands for each item.
- [While loops](loops.md#while-and-until-loops): Repeat commands while a condition is true.
- [Until loops](loops.md#while-and-until-loops): Repeat commands until a condition becomes true.

<!-- TODO: Double bracket command -->

A function definition creates a reusable block of code that can be invoked by name.
