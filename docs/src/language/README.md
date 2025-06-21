# Shell language

The shell interprets input as commands written in the shell language. The language has a syntax (how commands are written and structured) and semantics (how commands are executed). This page gives a brief overview of shell commands.

## Simple commands

A [simple command](commands/simple.md) is the most basic command type. It consists of a sequence of words that are not [reserved words](words/keywords.md) or [operators](words/index.html#tokens-and-operators). For example:

```sh
ls
```

or:

```sh
echo "Hello, world!"
```

Most simple commands run a utility—a program that performs a specific task. The first word is the utility name; the rest are arguments.

All words (except redirection operators) in a simple command are expanded before the utility runs. See [Words, tokens, and fields](words/index.html) for details on parsing and expansion.

You can use [parameters](parameters/index.html) to change command behavior dynamically. There are three types: [variables](parameters/variables.md), [special parameters](parameters/special.md), and [positional parameters](parameters/positional.md).

See [Simple commands](commands/simple.md) for more on assignments, redirections, and command search.

## Other commands

Other command types construct more complex behavior by combining commands. See [Commands](commands/index.html) for the full list. For example:

- Compound commands group commands, control execution, and handle conditions and loops. Examples: [`if`](commands/exit_status.md#if-commands), [`for`](commands/loops.md#for-loops), [`while`](commands/loops.md#while-and-until-loops), [`case`](commands/case.md).
- [Pipelines](commands/pipelines.md) connect the output of one command to the input of another, letting you chain commands.
- [And-or lists](commands/exit_status.md#and-or-lists) control execution flow based on command success or failure.
- [Lists](commands/lists.md) let you run multiple commands in sequence or in parallel.

## Functions

[Functions](functions.md) are reusable blocks of code you can define and call in the shell. They help organize scripts and interactive sessions.

## Redirections

Redirections control where command input and output go. Use them to save output to files or read input from files. See the File descriptors and redirections section for more.

## Aliases

Aliases are shortcuts for longer commands or command sequences. They let you create custom names for commands you use often. See the Aliases section for details.
