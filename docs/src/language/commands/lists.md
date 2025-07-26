# Lists and asynchronous commands

A **list** is a sequence of [and-or lists](exit_status.md#and-or-lists) separated by semicolons (`;`) or ampersands (`&`). Lists let you write multiple commands on a single line, and control whether they run synchronously or asynchronously.

## Synchronous commands

When an and-or list is separated by a semicolon (`;`), it runs synchronously: the shell waits for the command to finish before running the next one.

```shell
$ echo "First command"; echo "Second command";
First command
Second command
```

The semicolon can be omitted after the last command:

```shell
$ echo "First command"; echo "Second command"
First command
Second command
```

## Asynchronous commands

When an and-or list is separated by an ampersand (`&`), it runs asynchronously: the shell does not wait for the command to finish before running the next one.

```shell,no_run
$ echo "First async command" & echo "Second async command" & echo "Synchronous command"
Second async command
Synchronous command
First async command
```

Here, the commands run in parallel, so their output may appear in any order.

In an interactive shell, starting an asynchronous command prints its job number and process ID:

```shell,no_run
$ echo "Async command" &
[1] 12345
Async command
```

Because the shell does not wait for asynchronous commands, they may keep running while the shell reads new commands or even after the shell exits. To wait for them to finish, use the `wait` utility (see below).

### Input redirection

By default, an asynchronous command's standard input is redirected to `/dev/null` to prevent it from interfering with synchronous commands that read from standard input. This does not apply in job-controlling shells.

```shell
$ echo Input | {
>     cat &
>     read -r line
>     echo "Read line: $line"
> }
Read line: Input
```

In this example, the asynchronous `cat` reads from `/dev/null`, while `read` reads from standard input.

### The `!` special parameter

The `!` [special parameter](../parameters/special.md) gives the process ID of the last asynchronous command started in the shell. This is useful for tracking or waiting for background jobs.

### The `wait` utility

The `wait` utility waits for asynchronous commands to finish. With no operands, it waits for all asynchronous commands started in the current shell. With operands, it waits for the specified process IDs.

```shell,hidelines=#
#$ mkdir $$ && cd $$ || exit
$ echo "Async command" > async.txt &
$ echo "Synchronous command"
Synchronous command
$ wait $!
$ cat async.txt
Async command
```

Here, the shell starts an asynchronous command that writes to a file. `wait $!` waits for it to finish before reading the file.

### Job control

In yash-rs, all asynchronous commands start as background jobs. If the `monitor` shell option is enabled, you can use job control commands to manage these jobs. See the [job control documentation](../../interactive/job_control.md) for details.
