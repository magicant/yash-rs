# Simple commands

**Simple commands** are the basic building blocks of shell commands. They consist of command line words, assignments, and redirections.

## Outline

Despite the name, the behavior of simple commands is quite complex. This section outlines the key aspects of simple commands before diving into the details later.

Most simple commands run a utility, which is a program that performs a specific task. A simple command that runs a utility contains a word that specifies the utility name, followed by zero or more words that specify arguments to the utility:

```shell
$ echo "Hello!"
Hello!
```

The words are [expanded](../words/index.html#word-expansion) before the utility runs.

A simple command can assign values to [variables](../parameters/variables.md). To perform an assignment, join the variable name and value with an equals sign (`=`) without spaces:

```shell
$ greeting="Hello!"
$ echo "$greeting"
Hello!
```

If assignments are used with utility-invoking words, they must appear before the utility name and they (usually) affect only that utility invocation:

```shell,no_run
$ TZ='America/Los_Angeles' date
Sun Jun  1 06:49:30 AM PDT 2025
$ TZ='Asia/Tokyo' date
Sun Jun  1 10:49:30 PM JST 2025
```

A simple command can also redirect input and output using redirection operators. For example, to redirect the output of a command to a file, use the `>` operator:

```shell
$ echo "Hello!" > output.txt
$ cat output.txt
Hello!
```

Redirections can appear anywhere in a simple command, but they are conventionally placed at the end.

## Syntax

The formal syntax of a simple command, written in [Extended Backus-Naur Form (EBNF)](https://en.wikipedia.org/wiki/Extended_Backus%E2%80%93Naur_form), is as follows:

```ebnf
simple_command            := normal_utility - reserved_word, [ normal_argument_part ] |
                             declaration_utility, [ declaration_argument_part ] |
                             assignment_part, [ utility_part ] |
                             "command", [ utility_part ];
utility_part              := normal_utility, [ normal_argument_part ] |
                             declaration_utility, [ declaration_argument_part ] |
                             "command", [ utility_part ];
assignment_part           := ( assignment_word | redirection ), [ assignment_part ];
assignment_word           := ? a word that starts with a literal variable name
                               immediately followed by an unquoted equals sign and
                               optionally followed by a value part ?;
command_name              := word - assignment_word;
declaration_utility       := "export" | "readonly" | "typeset";
normal_utility            := command_name - declaration_utility - "command";
normal_argument_part      := ( word | redirection ), [ normal_argument_part ];
declaration_argument_part := ( assignment_word | word - assignment_word | redirection ),
                             [ declaration_argument_part ];
```

As shown above:

- A simple command cannot start with a [reserved word](../words/keywords.md) unless it is [quoted](../words/quoting.md).
- An assignment word must start with a non-empty [variable name](../parameters/variables.md#variable-names), but the variable value can be empty.
- To treat a word containing an equals sign as a command name, quote the variable name or equal sign.
- Redirections can appear anywhere in a simple command.

There must be no space around the equals sign in an assignment word. If you need to include spaces in the value, quote them:

```shell
$ greeting="Hello, world!"
$ echo "$greeting"
Hello, world!
```

The utility names `export`, `readonly`, and `typeset` are called **declaration utilities**; when one of these is used as a command name, following argument words are parsed as assignment words if possible, or as normal words otherwise. This affects how the argument words are expanded. The utility name `command` is also special; it delegates to the next word the determination of whether it is a declaration utility or a normal utility. (More utility names can be regarded as declaration utilities in the future.)

## Semantics

A simple command is executed by the shell in the following steps:

1. Command words (name and arguments) are [expanded](../words/index.html#word-expansion) in the order they appear, if any.
    - If the command name word is a declaration utility, argument words that have the form of an assignment word are expanded in the same manner as an assignment word (see below), and the rest are expanded as normal words. Otherwise, all arguments are expanded as normal words.
    - Since command words are expanded before assignments, command words are not affected by assignments in the same simple command.
    - Expansions in assignments and redirections are not yet performed in this step.
    - The result of expansion is a sequence of words, specifically called **fields**.
    - If expansion fails, the error is reported and the command execution is aborted.
2. Redirections are performed, if any. in the order they appear.
    - If the previous step produced any fields, the redirections are processed in the current shell environment. If a redirection fails, the error is reported and the command execution is aborted.
    - If there are no fields, the redirections are processed in a subshell. This means that the redirections do not affect the current shell environment and that errors in redirections are reported but do not abort the command execution.
3. Assignments are performed, if any, in the order they appear.
    - The value of each assignment is expanded and assigned to the variable in the current shell environment. See the [Defining variables](../parameters/variables.md#defining-variables) section for the basic assignment behavior.
    - The assigned variables are exported if there are any fields in the previous step or the `allexport` option is enabled.
    - If an assigned variable is read-only, the error is reported and the command execution is aborted.
4. If there are any fields, the shell determines the target to be executed based on the first field (the command name), following the steps described in the [Command search](#command-search) subsection below. <!-- TODO: #530 - Since this step is performed after the assignments, the command search can be affected by the assignments in the previous step. -->
5. The shell executes the target.
    - If the target is an external utility, it is executed in a subshell with the fields as arguments. If the `execve` system call fails with `ENOEXEC`, the shell tries to execute the target as a script in a new shell process.
    - If the target is a built-in, it is executed in the current shell environment with the fields (except the first) as arguments.
    - If the target is a function, it is executed in the current shell environment. When entering a function, the positional parameters are set to the fields (except the first). The positional parameters are restored to their previous values when the function returns.
    - If no target was found in the previous step, the shell reports an error.
    - If there was no command name (the first field), the shell does not execute anything.

The assigned variables are removed unless the target was a special built-in or there were no fields resulting from the expansion, in which case the assigned variables persist after the command.
The effect of the redirections is canceled unless the target was the `exec` special built-in (or the `command` built-in executing `exec`), in which case the redirections persist after the command.

### Command search

**Command search** determines the target to be executed based on the command name (the first field of the simple command). The search follows these steps:

1. If the command name contains a slash (`/`), it is treated as a pathname to an executable file, concluding the search regardless of whether the file exists or is executable.
2. If the command name is a special built-in (like `exec` and `exit`), it is determined to be the target.
3. If the command name is a function, it is determined to be the target.
4. If the command name is a built-in other than a substitutive built-in, it is determined to be the target. <!-- TODO: reject elective and extension built-ins in POSIX mode -->
5. The shell searches for the command name in the directories listed in the `PATH` [variable](../parameters/variables.md). If found, the first matching executable regular file is a candidate target.
    - The value of `PATH` treated as a sequence of pathnames separated by colons (`:`). An empty pathname in `PATH` refers to the current directory. For example, in the simple command `PATH=/bin:/usr/bin: ls`, the shell searches for `ls` in `/bin`, then `/usr/bin`, and finally the current directory.
    - If `PATH` is an array, each element is treated as a pathname to search.
6. If a candidate target is found:
    - If the command name is a substitutive built-in (like `echo` and `pwd`), the built-in is determined to be the target.
    - Otherwise, the executable file is determined to be the target.
7. If no candidate target is found, the command search fails.

An executable file target is called an **external utility**.

### Exit status

- If the simple command executed a target, the exit status of the simple command is the exit status of the target.
- If there were no fields after expansion, the exit status of the simple command is the exit status of the last [command substitution](../words/command_substitution.md) executed in the simple command, or zero if there were no command substitutions.
- If the simple command was aborted due to an error in expansion, redirection, assignment, or command search, the exit status is non-zero. Specifically, the exit status is 127 if the command search failed, or 126 if the command search succeeded but the target could not be executed (e.g., unsupported file type or permission denied).
