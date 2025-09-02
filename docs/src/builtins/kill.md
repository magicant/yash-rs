# Kill built-in

The **`kill`** built-in sends a [signal](../environment/traps.md#what-are-signals) to processes.

## Synopsis

```sh
kill [-s SIGNAL|-n SIGNAL|-SIGNAL] target…
```

```sh
kill -l|-v [SIGNAL|exit_status]…
```

## Description

Without the `-l` or `-v` option, the built-in sends a signal to processes.

With the `-l` or `-v` option, the built-in lists signal names or
descriptions.

## Options

The **`-s`** or **`-n`** option specifies the signal to send. The signal name is case-insensitive, but must be specified without the `SIG` prefix. The default signal is `SIGTERM`. (Specifying a signal name with the `SIG` prefix may be allowed in the future.)

The signal may be specified as a number instead of a name. If the number
is zero, the built-in does not send a signal, but instead checks whether
the shell can send the signal to the target processes.

The obsolete syntax allows the signal name or number to be specified
directly after the hyphen like `-TERM` and `-15` instead of `-s TERM` and
`-n 15`.

The **`-l`** option lists signal names. The names are printed one per line,
without the `SIG` prefix.

The **`-v`** option implies and extends the `-l` option by displaying the signal number before each name. The output format for `-v` may be changed in the future to include signal descriptions as well.

## Operands

Without the `-l` or `-v` option, the built-in takes one or more operands
that specify the target processes. Each operand is one of the following:

- A positive decimal integer, which should be a process ID
- A negative decimal integer, which should be a negated process group ID
- `0`, which means the current process group
- `-1`, which means all processes
- A [job ID](../interactive/job_control.md#job-ids), which means the process group of the job

With the `-l` or `-v` option, the built-in may take operands that limit the
output to the specified signals. Each operand is one of the following:

- The [exit status](../language/commands/exit_status.md#exit-status) of a process that was terminated by a signal
- A signal number
- A signal name without the `SIG` prefix

Without operands, the `-l` and `-v` options list all signals.

## Errors

It is an error if:

- The `-l` or `-v` option is not specified and no target processes are
  specified.
- A specified signal is not supported by the shell.
- A specified target process does not exist.
- The target job specified by a job ID operand is not job-controlled, that is, [job control](../interactive/job_control.md) was off when the job was started.
- The signal cannot be sent to any of the target processes specified by an
  operand.
- An operand specified with the `-l` or `-v` option does not identify a
  supported signal.

## Exit status

The exit status is zero unless an error occurs. The exit status is zero if
the signal is sent to at least one process for each operand, even if the
signal cannot be sent to some of the processes.

## Usage notes

When a target is specified as a job ID, the built-in cannot tell whether
the job process group still exists. If the job process group has been
terminated and another process group has been created with the same
process group ID, the built-in will send the signal to the new process
group.

## Examples

Sending `SIGTERM` to the last started asynchronous command and showing the signal name represented by the exit status:

```shell,one_shot
$ sleep 10&
$ kill $!
$ wait $!
$ echo "Exit status $? corresponds to SIG$(kill -l $?)"
Exit status 399 corresponds to SIGTERM
```

Specifying a signal name and job ID:

```shell,one_shot
$ set -m
$ sleep 10&
$ kill -s INT %1
$ wait %1
$ echo "Exit status $? corresponds to SIG$(kill -l $?)"
Exit status 386 corresponds to SIGINT
```

Specifying a signal number:

```shell,one_shot
$ sleep 10&
$ kill -n 1 $!
$ wait $!
$ echo "Exit status $? corresponds to SIG$(kill -l $?)"
Exit status 385 corresponds to SIGHUP
```

The `--` separator is needed if the first operand starts with a hyphen (a negated process group ID):

```shell,one_shot
$ set -m
$ sleep 10&
$ kill -n 15 -- -$!
$ wait $!
$ echo "Exit status $? corresponds to SIG$(kill -l $?)"
Exit status 399 corresponds to SIGTERM
```

## Compatibility

The `kill` built-in is specified by POSIX.1-2024.

Specifying a signal number other than `0` to the `-s` option is a
non-standard extension.

Specifying a signal number to the `-n` option is a ksh extension. This
implementation also supports the `-n` option with a signal name.

The `kill -SIGNAL target…` form may not be parsed as expected by other
implementations when the signal name starts with an `s`. For example, `kill
-stop 123` may try to send the `SIGTOP` signal instead of the `SIGSTOP`
signal.

POSIX defines the following signal numbers:

- `0` (a dummy signal that can be used to check whether the shell can send
  a signal to a process)
- `1` (`SIGHUP`)
- `2` (`SIGINT`)
- `3` (`SIGQUIT`)
- `6` (`SIGABRT`)
- `9` (`SIGKILL`)
- `14` (`SIGALRM`)
- `15` (`SIGTERM`)

Other signal numbers are implementation-defined.

Using the `-l` option with more than one operand is a non-standard
extension. Specifying a signal name operand to the `-l` option is a
non-standard extension.

The `-v` option is a non-standard extension.

Some implementations print `0` or `EXIT` for `kill -l 0` or `kill -l EXIT`
while this implementation regards them as invalid operands.

On some systems, a signal may have more than one name. There seems to be no
consensus whether `kill -l` should print all names or just one name for each
signal. This implementation currently prints all names, but this behavior
may change in the future.
