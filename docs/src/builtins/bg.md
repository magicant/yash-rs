# Bg built-in

The **`bg`** built-in resumes suspended jobs in the background.

## Synopsis

```sh
bg [job_id…]
```

## Description

See [Job control](../interactive/job_control.md) for an overview of job control in yash-rs. The built-in resumes the specified jobs by sending the `SIGCONT` signal to them.

The (last) resumed job's process ID is set to the `!` [special parameter](../language/parameters/special.md).

## Options

None.

## Operands

Operands specify jobs to resume as [job IDs](../interactive/job_control.md#job-ids). If omitted, the built-in resumes the [current job](../interactive/job_control.md#current-and-previous-jobs).

(TODO: allow omitting the leading `%`)

## Standard output

The built-in writes the job number and name of each resumed job to the standard output.

## Errors

It is an error if:

- the specified job is not found,
- the specified job is not job-controlled, that is, job control was off when the job was started, or
- job control is off in the current shell environment.

## Exit status

Zero unless an error occurs.

## Examples

See [Job control](../interactive/job_control.md).

## Compatibility

Many implementations allow omitting the leading `%` from job IDs, though it is not required by POSIX.

Some implementations (including the previous version of yash, but not this version) regard it is an error to resume a job that has already terminated.
