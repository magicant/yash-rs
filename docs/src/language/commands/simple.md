# Simple commands

**Simple commands** are the basic building blocks of shell commands. They consist of command line words, assignments, and redirections.

## Outline

Despite the name, simple commands can have complex behavior. This section summarizes the main aspects before covering details.

Most simple commands run a **utility**—a program that performs a specific task. A simple command that runs a utility contains a word specifying the utility name, followed by zero or more words as arguments:

```shell
$ echo "Hello!"
Hello!
```

The words are [expanded](../words/index.html#word-expansion) before the utility runs.

A simple command can assign values to [variables](../parameters/variables.md). To assign a value, join the variable name and value with an equals sign (`=`) without spaces:

```shell
$ greeting="Hello!"
$ echo "$greeting"
Hello!
```

If assignments are used with utility-invoking words, they must appear before the utility name and they usually affect only that utility invocation:

```shell,no_run
$ TZ='America/Los_Angeles' date
Sun Jun  1 06:49:30 AM PDT 2025
$ TZ='Asia/Tokyo' date
Sun Jun  1 10:49:30 PM JST 2025
```

A simple command can also redirect input and output using redirection operators. For example, to redirect output to a file:

```shell
$ echo "Hello!" > output.txt
$ cat output.txt
Hello!
```

Redirections can appear anywhere in a simple command, but are typically placed at the end.

## Syntax

The formal syntax of a simple command, written in [Extended Backus-Naur Form (EBNF)](https://en.wikipedia.org/wiki/Extended_Backus%E2%80%93Naur_form):

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

Key points:

- A simple command cannot start with a [reserved word](../words/keywords.md) unless it is [quoted](../words/quoting.md).
- An assignment word must start with a non-empty [variable name](../parameters/variables.md#variable-names), but the value can be empty.
- Assignment words must come before a command name if present.
- To treat a word containing `=` as a command name, quote the variable name or the equals sign.
- Redirections can appear anywhere in a simple command.

There must be no space around the equals sign in an assignment word. If you need spaces in the value, quote them:

```shell
$ greeting="Hello, world!"
$ echo "$greeting"
Hello, world!
```

The utility names `export`, `readonly`, and `typeset` are **declaration utilities**; when used as a command name, following argument words are parsed as assignment words if possible, or as normal words otherwise. This affects how arguments are expanded. The utility name `command` is also special; it delegates to the next word the determination of whether it is a declaration utility or a normal utility. (More utility names may be treated as declaration utilities in the future.)

## Semantics

A simple command is executed by the shell in these steps:

1. Command words (name and arguments) are [expanded](../words/index.html#word-expansion) in order.
    - If the command name is a declaration utility, argument words that look like assignment words are expanded as assignments; the rest are expanded as normal words. Otherwise, all arguments are expanded as normal words.
    - Command words are expanded before assignments, so assignments do not affect command words in the same command.
    - Expansions in assignments and redirections are not yet performed in this step.
    - The result is a sequence of words called **fields**.
    - If expansion fails, the error is reported and the command is aborted.
2. [Redirections](../redirections/index.html) are performed, in order.
    - If there are any fields, redirections are processed in the current [shell environment]. If a redirection fails, the error is reported and the command is aborted.
    - If there are no fields, redirections are processed in a [subshell](../../environment/index.html#subshells). In this case, redirections do not affect the current shell, and errors are reported but do not abort the command.
3. Assignments are performed, in order.
    - Each assignment value is expanded and assigned to the variable in the current shell environment. See [Defining variables](../parameters/variables.md#defining-variables) for details.
    - Assigned variables are exported if there are any fields or if the [`allexport` option](../../environment/options.md#option-list) is enabled.
    - If an assigned variable is [read-only](../parameters/variables.md#read-only-variables), the error is reported and the command is aborted.
4. If there are any fields, the shell determines the target to execute based on the first field (the command name), as described in [Command search](#command-search) below. <!-- TODO: #530 - Since this step is performed after the assignments, the command search can be affected by the assignments in the previous step. -->
5. The shell executes the target:
    - If the target is an external utility, it is executed in a [subshell](../../environment/index.html#subshells) with the fields as arguments. If the `execve` call used to execute the target fails with `ENOEXEC`, the shell tries to execute it as a script in a new shell process.
    - If the target is a [built-in](../../builtins/index.html), it is executed in the current [shell environment] with the fields (except the first) as arguments.
    - If the target is a [function], it is executed in the current [shell environment]. When entering a function, [positional parameters](../parameters/positional.md) are set to the fields (except the first), and restored when the function returns.
    - If no target is found, the shell reports an error.
    - If there was no command name (the first field), nothing is executed.

Assigned variables are removed unless the target was a [special built-in] or there were no fields after expansion, in which case the assignments persist.
Redirections are canceled unless the target was the [`exec` special built-in](../../builtins/exec.md) (or the [`command` built-in](../../builtins/command.md) executing `exec`), in which case the redirections persist.

### Command search

**Command search** determines the target to execute based on the command name (the first field):

1. If the command name contains a slash (`/`), it is treated as a pathname to an executable file target, regardless of whether the file exists or is executable.
2. If the command name is a [special built-in] (like [`exec`](../../builtins/exec.md) or [`exit`](../../builtins/exit.md)), it is used as the target.
3. If the command name is a [function], it is used as the target.
4. If the command name is a [built-in] other than a [substitutive built-in], it is used as the target. <!-- TODO: reject elective and extension built-ins in POSIX mode -->
5. The shell searches for the command name in the directories listed in the `PATH` [variable](../parameters/variables.md). The first matching executable regular file is a candidate target.
    - The value of `PATH` is treated as a sequence of pathnames separated by colons (`:`). An empty pathname in `PATH` refers to the current [working directory](../../environment/working_directory.md). For example, in the simple command `PATH=/bin:/usr/bin: ls`, the shell searches for `ls` in `/bin`, then `/usr/bin`, and finally the current directory.
    - If `PATH` is an array, each element is a pathname to search.
6. If a candidate target is found:
    - If the command name is a [substitutive built-in] (like [`true`](../../builtins/true.md) or [`pwd`](../../builtins/pwd.md)), the built-in is used as the target.
    - Otherwise, the executable file is used as the target.
7. If no candidate target is found, the command search fails.

An executable file target is called an **external utility**.

### Exit status

- If a target was executed, the [exit status](../commands/exit_status.md#exit-status) of the simple command is the exit status of the target.
- If there were no fields after expansion, the exit status is that of the last [command substitution](../words/command_substitution.md) in the command, or zero if there were none.
- If the command was aborted due to an error before running a target, the exit status is non-zero. Specifically:
    - 127 if command search failed
    - 126 if the target was identified but could not be executed (e.g., unsupported file type or permission denied)

[built-in]: ../../builtins/index.html
[function]: ../functions.md
[shell environment]: ../../environment/index.html
[special built-in]: ../../builtins/index.html#special-built-ins
[substitutive built-in]: ../../builtins/index.html#substitutive-built-ins
