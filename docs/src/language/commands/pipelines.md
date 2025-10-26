# Pipelines

A **pipeline** is a sequence of commands connected by pipes (`|`). The output of each command is passed as input to the next, allowing you to chain commands for more complex tasks.

## Basic usage

The syntax for a pipeline is:

```sh
command1 | command2 | command3 …
```

For example, to list files and filter the output:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ > foo.txt > bar.txt > baz.png
$ ls | grep .txt
bar.txt
foo.txt
```

The `|` operator may be followed by linebreaks for readability:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ > foo.txt > bar.txt > baz.png
$ ls |
> grep .txt |
> wc -l
2
```

[Line continuation](../words/quoting.md#line-continuation) can also be used to split pipelines across multiple lines:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ > foo.txt > bar.txt > baz.png
$ ls \
> | grep .txt \
> | wc -l
2
```

If a pipeline contains only one command, the shell runs that command directly. For multiple commands, the shell creates a [subshell](../../environment/index.html#subshells) for each and connects them with pipes. Each command's [standard output](../redirections/index.html#what-are-file-descriptors) is connected to the next command's [standard input](../redirections/index.html#what-are-file-descriptors). The first command's input and the last command's output are not changed. All commands in the pipeline run concurrently.

The shell waits for all commands in the pipeline to finish before proceeding. In the future, yash-rs may only wait for commands needed to determine the pipeline's exit status. By default, the [exit status](exit_status.md#exit-status) of the pipeline is the exit status of the last command in the pipeline, but see the next section for how to change this behavior.

## Catching errors across pipeline components

By default, the exit status of a pipeline reflects only the last command, ignoring failures in earlier commands. To make the pipeline fail if any command fails, enable the `pipefail` [shell option](../../environment/options.md). With `pipefail`, the pipeline's exit status is that of the last command that returned a non-zero status, or zero if all returned zero. This helps catch errors in pipelines.

```shell
$ set +o pipefail
$ echo foo | ( cat; exit 42 ) | grep foo
foo
$ echo $?
0
```

```shell
$ set -o pipefail
$ echo foo | ( cat; exit 42 ) | grep foo
foo
$ echo $?
42
```

## Negation

You can negate a pipeline using the `!` [reserved word]:

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ ! ls | grep .zip
$ echo $?
0
```

This runs the pipeline and negates its exit status: if the status is 0 (success), it becomes 1 (failure); if non-zero (failure), it becomes 0 (success). This is useful for inverting the result of a command in a conditional.

Negation applies to the pipeline as a whole, not to individual commands. To negate a specific command, use [braces](grouping.md#braces):

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ ls | { ! grep .zip; } && echo "No zip files found"
No zip files found
```

Since `!` is a [reserved word], it must appear as a separate word:

```shell
$ !ls | grep .zip
error: cannot execute external utility "!ls"
 --> <stdin>:1:1
  |
1 | !ls | grep .zip
  | ^^^ utility "!ls" not found
```

## Compatibility

POSIX requires that a pipeline waits for the last command to finish before returning an exit status, and it is unspecified whether the shell waits for all commands in the pipeline to finish. yash-rs currently waits for all commands, but this may change in the future.

POSIX allows commands in a multi-command pipeline to be run in the current [shell environment](../../environment/index.html) rather than in subshells. Korn shell and zsh run the last command in the current shell environment, while yash-rs runs all commands in subshells.

Some shells like Korn shell and mksh assign special meanings to the `!` reserved word immediately followed by the `(` operator. For maximum compatibility, `!` and `(` should be separated by a space.

[reserved word]: ../words/keywords.md
