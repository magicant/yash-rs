# Jobs built-in

The **`jobs`** built-in reports job status.

## Synopsis

```sh
jobs [-lnprst] [job_id…]
```

## Description

The jobs built-in prints information about jobs the shell is currently
controlling, one line for each job. The results follow the
[format](yash_env::job::fmt) specified by the POSIX.

When the built-in reports a finished job (either exited or signaled), it
removes the job from the current execution environment.

## Options

### Format

By default, the results are printed in the standard format defined in the
[`yash_env::job::fmt`] module. You can use two options to change the output.

The **`-l`** (**`--verbose`**) option uses the alternate format, which
inserts the process ID before each job state. The **`-p`**
(**`--pgid-only`**) option only prints the process ID of each job.

### Filtering

TODO `-n`, `-r`, `-s`, `-t`

## Operands

Each operand is parsed as a [job ID](yash_env::job::id) that specifies which
job to report. If no operands are given, the built-in prints all jobs.

## Exit status

`ExitStatus::SUCCESS` or `ExitStatus::FAILURE` depending on the results

## Portability

The current implementation of this built-in removes finished jobs from the
environment after reporting all jobs. This behavior should not be relied
upon. The following script shows a "job not found" error in many other
shells because the built-in removes the job when processing the first
operand so the job is gone when the second is processed:

```sh
sleep 0&
jobs %sleep %sleep
```

When the built-in is used in a subshell, the built-in reports not only jobs
that were started in the subshell but also jobs that were started in the
parent shell. This behavior is not portable and is subject to change.

The POSIX standard only defines the `-l` and `-p` options. Other options are
non-portable extensions.

According to POSIX, the `-p` option takes precedence over the `-l` option.
In many other shells, however, the last specified one is effective.

A portable job ID must start with a `%`. If an operand does not have a
leading `%`, the built-in assumes one silently, which is not portable.
