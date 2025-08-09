# Wait built-in

The **`wait`** built-in waits for asynchronous jobs to finish.

## Synopsis

```sh
wait [job_id_or_process_id…]
```

## Description

If you specify one or more operands, the built-in waits for the specified
jobs to finish. Otherwise, the built-in waits for all existing asynchronous
jobs. If the jobs are already finished, the built-in returns without
waiting.

If you try to wait for a suspended job, the built-in will wait indefinitely
until the job is resumed and finished. Currently, there is no way to
cancel the wait.
(TODO: Add a way to cancel the wait)
(TODO: Add a way to treat a suspended job as if it were finished)

## Options

None

## Operands

An operand can be a job ID or decimal process ID, specifying which job to
wait for. A job ID must start with `%` and has the format described in the
[`yash_env::job::id`] module documentation. A process ID is a non-negative
decimal integer.

If there is no job matching the operand, the built-in assumes that the
job has already finished with exit status 127.

## Errors

The following error conditions causes the built-in to return a non-zero exit
status without waiting for any job:

- An operand is not a job ID or decimal process ID.
- A job ID matches more than one job.
- The shell receives a signal that has a [trap](yash_env::trap) action set.

The trap action for the signal is executed before the built-in returns.

## Exit status

If you specify one or more operands, the built-in returns the exit status of
the job specified by the last operand. If there is no operand, the exit
status is 0 regardless of the awaited jobs.

If the built-in was interrupted by a signal, the exit status indicates the
signal.

The exit status is between 1 and 126 (inclusive) for any other error.

## Compatibility

The wait built-in is contained in the POSIX standard.

The exact value of an exit status resulting from a signal is
implementation-dependent.

Many existing shells behave differently on various errors. POSIX requires
that an unknown process ID be treated as a process that has already exited
with exit status 127, but the behavior for other errors should not be
considered portable.
