# Jobs built-in

The **`jobs`** built-in reports job status.

## Synopsis

```sh
jobs [-lp] [job_id…]
```

## Description

See [Job control](../interactive/job_control.md) for an overview of job control in yash-rs. The `jobs` built-in prints information about jobs the shell is currently controlling, one line for each job. The results follow the format specified by the POSIX.

When the built-in reports a finished job (either exited or signaled), it
removes the job from the [job list](../interactive/job_control.md#job-list).

### Format

An example output of the built-in in the default format is:

```text
[1] - Running              cargo build
[2] + Stopped(SIGTSTP)     vim
[3]   Done                 rm -rf /tmp/foo
```

The first column is the job number in brackets. The second column indicates the [current job](../interactive/job_control.md#current-and-previous-jobs) (`+`) or the [previous job](../interactive/job_control.md#current-and-previous-jobs) (`-`). The third column is the job state, which can be one of:

- `Running`: the job is running in the background
- `Stopped(signal)`: the job is stopped by *signal*
- `Done`: the job has finished with an exit status of zero
- `Done(n)`: the job has finished with non-zero exit status *n*
- `Killed(signal)`: the job has been killed by *signal*
- `Killed(signal: core dumped)`: the job has been killed by *signal* and a core dump was produced

The last column is the command line of the job.

## Options

### Format

You can use two options to change the output.

The **`-l`** (**`--verbose`**) option uses the alternate format, which
inserts the process ID before each job state. The **`-p`**
(**`--pgid-only`**) option only prints the process ID of each job.

### Filtering

TODO `-n`, `-r`, `-s`, `-t`

## Operands

Each operand is parsed as a [job ID](../interactive/job_control.md#job-ids) that specifies which
job to report. If no operands are given, the built-in prints all jobs.

## Errors

If an operand does not specify a valid job, the built-in prints an error message to the standard error and returns a non-zero exit status. An ambiguous job ID (matching multiple jobs) is also an error.

## Exit status

Zero if successful, non-zero if an error occurred.

## Examples

[Job list](../interactive/job_control.md#job-list) includes an example of using the `jobs` built-in to list jobs.

The built-in with different arguments:

```shell,no_run
$ vim
[1] + Stopped(SIGTSTP)     vim
$ sleep 60 && echo "1 minute elapsed!"&
[2] 38776
$ jobs
[1] + Stopped(SIGTSTP)     vim
[2] - Running              sleep 60 && echo "1 minute elapsed!"
$ jobs -l
[1] + 37424 Stopped(SIGTSTP)     vim
[2] - 38776 Running              sleep 60 && echo "1 minute elapsed!"
$ jobs -p %2
38776
```

## Compatibility

The output format may differ between shells. Specifically:

- For a job stopped by `SIGTSTP`, other shells may omit the signal name and simply print `Stopped`.
- Other shells may report stopped jobs as `Suspended` instead of `Stopped`.
- The job state format for jobs terminated by a signal is implementation-defined.
- With the `-l` option, shells that manage more than one process per job may print an additional line containing the process ID and name for each process in the job.

The current implementation of this built-in removes finished jobs from the
job list after reporting all jobs. This behavior should not be relied
upon. The following script shows a "job not found" error in many other
shells because the built-in removes the job when processing the first
operand so the job is gone when the second is processed:

<!-- markdownlint-disable MD014 -->
```shell,no_run
$ sleep 0&
$ jobs %sleep %sleep
```
<!-- markdownlint-enable MD014 -->

When the built-in is used in a subshell, the built-in reports not only jobs
that were started in the subshell but also jobs that were started in the
parent shell. This behavior is not portable and is subject to change.

The POSIX standard only defines the `-l` and `-p` options. Other options are
non-portable extensions.

According to POSIX, the `-p` option takes precedence over the `-l` option.
In many other shells, however, the last specified one is effective.

A portable job ID must start with a `%`. If an operand does not have a
leading `%`, the built-in assumes one silently, which is not portable.
