# Wait built-in

The **`wait`** built-in waits for jobs to finish.

## Synopsis

```sh
wait [job_id_or_process_id…]
```

## Description

See [Job control](../interactive/job_control.md) for an overview of job control in yash-rs, though this built-in can also be used for [asynchronous commands](../language/commands/lists.md#asynchronous-commands) without job control. If you specify one or more operands, the built-in waits for the specified jobs to finish. Otherwise, the built-in waits for all existing jobs. If the jobs are already finished, the built-in returns without waiting.

If you attempt to wait for a suspended job, the built-in will wait indefinitely until the job is resumed and completes. Currently, there is no way to cancel a wait in progress. When job control is enabled, it is often preferable to use [`fg`](fg.md) instead of `wait`, as `fg` allows you to interact with the job—including suspending or interrupting it.

In the future, the shell may provide a way to cancel a wait in progress or treat a suspended job as if it were finished.

## Options

None.

## Operands

An operand can be a [job ID] or decimal process ID, specifying which job to wait for. A process ID is a non-negative decimal integer.

If there is no job matching the operand, the built-in assumes that the job has already finished with exit status 127.

## Errors

The following error conditions causes the built-in to return a non-zero exit status without waiting for any job:

- An operand is not a [job ID] or decimal process ID.
- A [job ID] matches more than one job.

If the shell receives a signal that has a [trap](../environment/traps.md#what-are-traps) action set, the trap action is executed and the built-in returns immediately.

## Exit status

If you specify one or more operands, the built-in returns the exit status of the job specified by the last operand. If there is no operand, the exit status is 0 regardless of the awaited jobs.

If the built-in was interrupted by a signal, the [exit status](../language/commands/exit_status.md#exit-status) indicates the signal.

The exit status is between 1 and 126 (inclusive) for any other error.

## Examples

See [The `wait` utility](../language/commands/lists.md#the-wait-utility) for an example of using the `wait` built-in without job control.

Using `wait` with job control and examining the exit status:

```shell,one_shot
$ set -m
$ sleep 10&
$ kill %
$ wait %
$ echo "Exit status $? corresponds to SIG$(kill -l $?)"
Exit status 399 corresponds to SIGTERM
```

[Subshells](../environment/index.html#subshells) cannot wait for jobs in the parent shell environment:

```shell,one_shot,hidelines=#
$ sleep 10&
$ (wait %)
$ echo $?
127
#$ kill $!
```

In the above example, `wait` treats the job `%` as an unknown job and returns exit status 127.

## Compatibility

The `wait` built-in is specified in POSIX.1-2024.

POSIX does not require the `wait` built-in to conform to the [Utility Syntax Guidelines](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap12.html#tag_12_02), which means portable scripts cannot use any options or the `--` separator for the built-in.

Many existing shells behave differently on various errors. POSIX requires that an unknown process ID be treated as a process that has already exited with exit status 127, but the behavior for other errors should not be considered portable.

[Job ID]: ../interactive/job_control.md#job-ids
