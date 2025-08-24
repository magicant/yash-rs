# Job control

**Job control** lets users selectively stop (suspend) commands and resume them later. It also allows users to interrupt running commands.

Job control is intended for use in [interactive] shells. Unless otherwise noted, the descriptions below assume the shell is interactive.

## Overview

Let's first get a general idea of job control.

### Why is job control useful?

Suppose you start a command to download a file, but realize the network is down and want to cancel the download. With job control, you can interrupt the command by pressing `Ctrl-C`, which usually terminates it:

```shell,no_run
$ curl 'http://example.com/largefile.zip' > largefile.zip
^C
$
```

Job control also lets you move running commands between the foreground and background, making it easier to manage multiple tasks. For example, suppose you run a command to remove many files:

<!-- markdownlint-disable MD014 -->
```shell,no_run
$ rm -rf ~/.my_cache_files
```
<!-- markdownlint-enable MD014 -->

If the command is taking too long, you might wish you had started it as an [asynchronous command] so you could run other commands at the same time. With job control, you can turn this running command into an asynchronous one without stopping and restarting it. First, press `Ctrl-Z` to suspend the command and return to the [prompt]. The shell displays a message indicating the command has been stopped:

```shell,no_run
^Z[1] + Stopped(SIGTSTP)     rm -rf ~/.my_cache_files
```

You can then resume the command in the background with the [`bg` built-in]:

```shell,no_run
$ bg
[1] rm -rf ~/.my_cache_files
```

Now the command runs asynchronously, and you can continue using the shell for other tasks. If you want to bring the command back to the foreground (synchronous execution), use the [`fg` built-in]:

```shell,no_run
$ fg
rm -rf ~/.my_cache_files
```

Another common scenario is when you use an editor to work on source code and want to build and test your code while keeping the editor open. You can suspend the editor with `Ctrl-Z`, run your build command, and then return to the editor with [`fg`]:

```shell,no_run
$ vi main.rs
^Z[1] + Stopped(SIGTSTP)     vi main.rs
$ cargo build
$ fg
vi main.rs
```

(This example only shows the shell output. In practice, you would also see the editor screen and the build command's output.)

### How job control works

The shell implements job control using the operating system's process management features. It manages processes and their states, allowing users to control their execution.

When the shell starts a [subshell], it runs the subshell in a separate process. This process is placed in a new process group, so any processes created during the subshell's execution can be managed together. Process groups allow the shell and other utilities to send [signals](../environment/traps.md#what-are-signals) to all relevant processes at once.

If the subshell runs synchronously (in the foreground), the shell sets its process group as the terminal's foreground process group. This lets you interact with the subshell's processes while they're running. When you press `Ctrl-C` or `Ctrl-Z`, the terminal sends a `SIGINT` or `SIGTSTP` signal to the foreground process group. Typically, `SIGINT` terminates a process, and `SIGTSTP` suspends it.

When a foreground process is suspended, the shell displays a message and returns to the [command prompt]. The shell keeps a list of remaining subshells (jobs) so you can manage them later. When you use the [`fg` built-in], the shell makes the specified job the foreground process group again and sends it a `SIGCONT` signal to resume execution. If you use [`bg`], the shell sends `SIGCONT` but leaves the job running in the background.

All commands in a [pipeline] run in the same process group, so you can manage the entire pipeline as a single job.

When the shell is reading input, it makes itself the terminal's foreground process group. This means key sequences like `Ctrl-C` and `Ctrl-Z` send signals to the shell itself. However, the shell ignores these signals to avoid being interrupted or suspended unintentionally.

## Enabling job control

By default, job control is enabled only if the shell is [interactive]. You can enable or disable job control at startup or during a shell session by specifying the `monitor` [shell option](../environment/options.md):

```sh
yash3 -o monitor
```

## Creating and managing jobs

Job control is complex. yash-rs implements it mostly according to the POSIX.1-2024 standard, with some deviations. Non-POSIX behavior is marked with ⚠️.

### Job control concepts

A **process** is a running instance of a program, such as the shell or an external utility. Each process belongs to a **process group**, which is a collection of processes managed together. Each process group belongs to a **session**, which is a collection of process groups.

When a process creates another process, the new process is its **child process**, and the original is the **parent process**. Child processes inherit certain attributes, such as process group and session, but can also create new ones.

In the context of job control, a **terminal** is an abstract interface managed by the operating system, which provides the necessary mechanisms for shells to implement job control. A terminal can be associated with a session, making it a **controlling terminal**. A process group can be selected as the terminal's **foreground process group**, which receives signals from key sequences like `Ctrl-C` and `Ctrl-Z`. Other process groups in the session are **background process groups**.

For this document, we assume all terminals are controlling terminals, since non-controlling terminals aren't useful for job control.

A **job** is a [subshell] implemented as a child process of the shell. (⚠️This differs from POSIX, which uses "job" for a broader set of commands, including [lists](../language/commands/lists.md).) Each job has a unique **job number**, a positive integer assigned by the shell when the job is created. The shell maintains a **job list** with information about each job's number, status, etc.

A **job ID** starts with `%` and is used to specify jobs in built-ins like [`fg`], [`bg`], and [`jobs`]. For example, `%1` refers to job number 1.

### Subshells and process groups

When job control is enabled, the shell manages each [subshell] as a job in a new process group, allowing independent control. A multi-command [pipeline] is treated as a single job, with all commands in the same process group. ⚠️Subshells created for [command substitutions](../language/words/command_substitution.md) are not treated as jobs and do not create new process groups, because yash-rs does not support suspending and resuming entire commands containing command substitutions.

Job control does not affect nested subshells recursively. However, if a subshell starts another shell that supports job control, that shell can manage jobs independently.

You can view job process groups using the `ps` utility:

```shell,no_run
$ sleep 60 && echo "1 minute elapsed!"&
[1] 10068
$ ps -j
    PID    PGID     SID TTY          TIME CMD
  10012   10012   10012 pts/1    00:00:00 yash3
  10068   10068   10012 pts/1    00:00:00 yash3
  10069   10068   10012 pts/1    00:00:00 sleep
  10076   10076   10012 pts/1    00:00:00 ps
```

### Foreground and background jobs

Unless starting an [asynchronous command], the shell runs jobs as the terminal's foreground process group. This directs signals from key sequences like `Ctrl-C` and `Ctrl-Z` to the job, not the shell or background jobs.

For example, pressing `Ctrl-C` interrupts a foreground job (the signal is invisible, but `^C` shows you pressed `Ctrl-C`):

```shell,no_run
$ sleep 60
^C$ 
```

When a foreground job terminates or suspends, the shell returns itself to the foreground so it can continue running commands and reading input. The shell can only examine the status of its direct child processes; descendant processes do not affect job control.

Here's how to suspend a foreground job with `Ctrl-Z` (`^Z` shows you pressed `Ctrl-Z`):

```shell,no_run
$ sleep 60
^Z[1] + Stopped(SIGTSTP)     sleep 60
$ 
```

An [asynchronous command] creates a background job, which runs alongside the shell and other jobs. The shell shows the job number and process (group) ID when the background job is created:

```shell,no_run
$ sleep 60&
[1] 10068
$ 
```

Background jobs are not affected by `Ctrl-C` or `Ctrl-Z`. To send signals to background jobs, use the `kill` built-in (see [Signaling jobs](#signaling-jobs)). You can also bring a background job to the foreground with [`fg`] and then use `Ctrl-C` or `Ctrl-Z`.

### Suspending foreground jobs

Pressing `Ctrl-Z` sends a `SIGTSTP` signal to the foreground process group. Processes may respond differently, but typically suspend execution.

When a foreground job suspends, the shell displays a message and discards any pending commands that have been read but not yet executed. This prevents the shell from running commands that might depend on the suspended job's result. (⚠️POSIX.1-2024 allows discarding only up to the next asynchronous command, but yash-rs discards all pending commands.)

For example, `sleep` is suspended and the following `echo` is discarded:

```shell,no_run
$ sleep 60 && echo "1 minute elapsed!"
^Z[1] + Stopped(SIGTSTP)     sleep 60
$ 
```

To avoid discarding remaining commands, run the sequence in a subshell. Here, the subshell is suspended during `sleep`, and `echo` runs after `sleep` resumes and finishes:

```shell,no_run
$ (sleep 60 && echo "1 minute elapsed!")
^Z[1] + Stopped(SIGTSTP)     sleep 60 && echo "1 minute elapsed!"
$ fg
sleep 60 && echo "1 minute elapsed!"
1 minute elapsed!
```

After suspension, the `?` [special parameter] shows the [exit status] of the suspended job as if it had been terminated by the signal that suspended it:

```shell,no_run
$ sleep 60
^Z[1] + Stopped(SIGTSTP)     sleep 60
$ echo "Exit status $? corresponds to SIG$(kill -l $?)"
Exit status 404 corresponds to SIGTSTP
```

### Resuming jobs

The [`fg` built-in] brings a job to the foreground and sends it a `SIGCONT` signal to resume execution. The job continues as the terminal's foreground process group, letting you interact with it again:

```shell,no_run
$ sleep 60 && echo "1 minute elapsed!"&
[1] 10051
$ fg
sleep 60 && echo "1 minute elapsed!"
^Z[1] + Stopped(SIGTSTP)     sleep 60 && echo "1 minute elapsed!"
$ fg
sleep 60 && echo "1 minute elapsed!"
1 minute elapsed!
```

The [`bg` built-in] sends `SIGCONT` to a job without bringing it to the foreground, letting it continue in the background while you use the shell for other tasks.

For example, `echo` prints a message while the shell is in the foreground:

```shell,no_run
$ (sleep 60 && echo "1 minute elapsed!")
^Z[1] + Stopped(SIGTSTP)     sleep 60 && echo "1 minute elapsed!"
$ bg
[1] sleep 60 && echo "1 minute elapsed!"
$ echo "Background job running"
Background job running
$ 1 minute elapsed!
```

### Signaling jobs

The [`kill` built-in] sends a signal to a process or process group. It accepts job IDs ([see below](#job-ids)) to specify jobs as targets. This allows you to control jobs at a low level, such as suspending or terminating them. For example, use `kill -s STOP %1` to suspend job 1, or `kill -s KILL %2` to terminate job 2:

```shell,no_run
$ sleep 60 && echo "1 minute elapsed!"&
[1] 10053
$ kill %1
[1] + Killed(SIGTERM)      sleep 60 && echo "1 minute elapsed!"
$ 
```

<!--
## Detaching jobs

TODO: disown
-->

## Job list

The job list includes each job's number, process (group) ID, status, and command string. The shell updates this list as jobs are created, suspended, resumed, or terminated. The process group ID of a job equals the process ID of its main process, so they are not distinguished in the job list.

Use the [`jobs` built-in] to display the current job list:

```shell,no_run
$ rm -r foo& rm -r bar& rm -r baz&
[1] 10055
[2] 10056
[3] 10057
$ jobs
[1] + Running              rm -r foo
[2] - Running              rm -r bar
[3]   Running              rm -r baz
```

When a foreground job terminates, the shell removes it from the job list. If a job terminates in the background, the shell keeps it in the list so you can see its status and retrieve its [exit status] later. Such jobs are removed when their result is retrieved using [`jobs`](../builtins/jobs.md) or [`wait`](../builtins/wait.md).

### Job numbers

When a job is created, the shell assigns it a unique job number, regardless of whether job control is enabled. Job numbers are assigned sequentially, starting from 1. After a job is removed, its number may be reused.

### Current and previous jobs

The shell automatically selects two jobs as the **current job** and **previous job** from the job list. These can be referred to with special job IDs ([see below](#job-ids)). Some built-ins operate on the current job by default, making it easy to specify jobs without typing a job number or command string.

In job IDs and [`jobs`] output, the current job is marked with `+`, and the previous job with `-`.

The current job is usually the most recently suspended job, or another job if none are suspended. When a job is suspended, it becomes the current job, and the previous current job becomes the previous job. When a suspended job is resumed or removed, the current and previous jobs are updated so the current job is always a suspended job if any exist, and the previous job is another suspended job if possible. If there is only one job, there is no previous job. These rules ensure built-ins like [`fg`] and [`bg`] operate on the most relevant jobs by default.

### Job IDs

[Built-in utilities](../builtins/index.html) that operate on jobs use job IDs to specify them. A job ID matches one of these formats:

- **`%`**, **`%%`**, or **`%+`**: the current job.
- **`%-`**: the previous job.
- **`%n`**: job number `n`.
- **`%foo`**: job with a command string starting with `foo`.
- **`%?foo`**: job with a command string containing `foo`.

### Job status change notifications

When a background job's status changes (suspended, resumed, or terminated), the shell automatically notifies you before the next [command prompt], so you can see job status changes without checking manually. The notification format matches the [`jobs` built-in] output.

```shell,no_run
$ rm -r foo& # remove a directory in the background
[1] 10059
$ rm -r bar # remove another directory in the foreground
[1] - Done                 rm -r foo
$ 
```

In this example, the `rm -r foo` job finishes while `rm -r bar` runs in the foreground. The background job's status change is automatically shown before the next prompt.

Note that automatic notifications do not remove the reported job from the job list; jobs are only removed after their status is retrieved using the [`jobs`] or [`wait`] built-ins.

## Additional details

The following sections cover special cases and extra features of job control you may not need in everyday use.

### Terminal setting management

⚠️Not yet implemented in yash-rs: Some utilities, like `less` and `vi`, change terminal settings for interactive use and complex UI. If suspended, they may leave the terminal in a state unsuitable for other utilities to run. To prevent this, the shell should restore the terminal settings when a foreground job is suspended, and again when the job is resumed in the foreground.

### Job control in non-interactive shells

You can enable job control in non-[interactive] shells, but it's rarely useful. Job control is mainly for interactive use, where users manage jobs dynamically. In non-interactive shells, there's no user interaction, so features like suspending and resuming jobs don't apply.

When job control is enabled in a non-interactive shell:

- The shell does not ignore `SIGINT`, `SIGTSTP`, or other job control signals by default. The shell itself may be interrupted or suspended with `Ctrl-C` or `Ctrl-Z`.
- The shell does not automatically notify you of job status changes. You must use the [`jobs` built-in] to check status.

### Jobs without job control

Each [asynchronous command] started when job control is disabled is also managed as a job, but runs in the same process group as the shell. Signals from key sequences like `Ctrl-C` and `Ctrl-Z` are sent to the whole process group, including the shell and the asynchronous command. This means jobs cannot be interrupted, suspended, or resumed independently. The shell still assigns job numbers and maintains the job list so you can see status and retrieve [exit status] later.

### Background shells

When a shell starts job control in the background, it suspends itself until brought to the foreground by another process. This prevents the shell from interfering with the current foreground process group. (⚠️POSIX.1-2024 requires using `SIGTTIN` for this, but yash-rs uses `SIGTTOU` instead. See [Issue #421](https://github.com/magicant/yash-rs/issues/421#issuecomment-2717123069) for details.)

⚠️POSIX.1-2024 requires the shell to become a process group leader—the initial process in a process group—when starting job control. Yash-rs does not currently implement this. See [Issue #483](https://github.com/magicant/yash-rs/issues/483) for why this is not straightforward.

### Configuring key sequences for signals

You can configure key sequences that send signals to the foreground process group using the `stty` utility. The table below shows parameter names, default key sequences, and corresponding signals:

| Parameter | Key      | Signal               |
|-----------|----------|----------------------|
| `intr`    | `Ctrl-C` | `SIGINT` (interrupt) |
| `susp`    | `Ctrl-Z` | `SIGTSTP` (suspend)  |
| `quit`    | `Ctrl-\` | `SIGQUIT` (quit)     |

For example, to change the `intr` key to `Ctrl-X`:

<!-- markdownlint-disable MD014 -->
```shell,no_run
$ stty intr ^X
```
<!-- markdownlint-enable MD014 -->

If your terminal uses different key sequences, press the appropriate keys instead of `Ctrl-C` or `Ctrl-Z` to send signals to the foreground process group.

View the current configuration with `stty -a`.

## Compatibility

POSIX.1-2024 defines job control but allows for implementation-defined behavior in many areas. Yash-rs follows the standard closely, with some deviations (marked with ⚠️). Job control is complex, and implementations differ. Perfect POSIX compliance is not expected in any shell, including yash-rs.

The job ID `%` is a common extension to POSIX.1-2024. The strictly portable way to refer to the current job is `%%` or `%+`.

In yash-rs, jobs are currently defined as subshells, whereas POSIX.1-2024 treats a wider range of commands as jobs. At present, yash-rs does not support suspending [built-ins](../builtins/index.html), since they run within the current shell environment rather than in a subshell. Future versions of yash-rs may expand the definition of jobs to include this capability.

[asynchronous command]: ../language/commands/lists.md#asynchronous-commands
[`bg`]: ../builtins/bg.md
[`bg` built-in]: ../builtins/bg.md
[command prompt]: prompt.md
[exit status]: ../language/commands/exit_status.md
[`fg`]: ../builtins/fg.md
[`fg` built-in]: ../builtins/fg.md
[interactive]: index.html
[`jobs`]: ../builtins/jobs.md
[`jobs` built-in]: ../builtins/jobs.md
[`kill` built-in]: ../builtins/kill.md
[pipeline]: ../language/commands/pipelines.md
[prompt]: prompt.md
[special parameter]: ../language/parameters/special.md
[subshell]: ../environment/index.html#subshells
[`wait`]: ../builtins/wait.md
