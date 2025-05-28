# Shell language

The shell interprets input as commands written in the shell language. The language has a syntax (how commands are written and structured) and semantics (how commands are executed). This page gives a brief overview of shell commands.

## Simple commands

A simple command is a sequence of words that are not reserved words or operators. For example:

```sh
ls
```

or:

```sh
echo "Hello, world!"
```

Most simple commands run a utility—a program that performs a specific task. The first word is the utility name; the rest are arguments.

All words (except redirection operators) in a simple command are expanded before the utility runs. See [Words, tokens, and fields](words/README.md) for details on parsing and expansion.

You can use [parameters](parameters/README.md) to change command behavior dynamically. See the Assignment section for how to define variables.

See the Simple command details section for more on how simple commands work, including word expansion, assignment, and redirection.

## Compound commands

Compound commands group commands, control execution, and handle conditions and loops. Examples include `if`, `for`, `while`, and `case`. Compound commands can contain multiple simple commands and be nested. See the Compound commands section for details.

## Pipelines and lists

Pipelines connect the output of one command to the input of another, letting you chain commands. A list is a sequence of commands separated by operators like `;`, `&&`, or `||`. See the Pipelines and Lists sections for usage.

## Functions

Functions are reusable blocks of code you can define and call in the shell. They help organize scripts and interactive sessions. Functions can take parameters. See the Functions section for details.

## Redirections

Redirections control where command input and output go. Use them to save output to files or read input from files. See the File descriptors and redirections section for more.

## Aliases

Aliases are shortcuts for longer commands or command sequences. They let you create custom names for commands you use often. See the Aliases section for details.
