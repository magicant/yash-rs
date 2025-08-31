# Fg built-in

The **`fg`** resumes a suspended job in the foreground.

## Synopsis

```sh
fg [job_id]
```

## Description

See [Job control](../interactive/job_control.md) for an overview of job control in yash-rs. The built-in brings the specified job to the foreground and resumes its execution by sending the `SIGCONT` signal to it. The built-in then waits for the job to finish (or suspend again).

If the resumed job finishes, it is removed from the [job list](../interactive/job_control.md#job-list). If the job gets suspended again, it is set as the [current job](../interactive/job_control.md#current-and-previous-jobs).

## Options

None.

## Operands

Operand *job_id* specifies which job to resume. See [Job IDs](../interactive/job_control.md#job-ids) for the operand format. If omitted, the built-in resumes the [current job](../interactive/job_control.md#current-and-previous-jobs).

(TODO: allow omitting the leading `%`)
(TODO: allow multiple operands)

## Standard output

The built-in writes the selected job name to the standard output.

(TODO: print the job number as well)

## Errors

It is an error if:

- the specified job is not found,
- the specified job is not job-controlled, that is, job control was off when the job was started, or
- job control is off in the current shell environment.

## Exit status

The built-in returns with the exit status of the resumed job. If the job is suspended, the exit status is as if the job had been terminated with the signal that suspended it. (See also [Suspending foreground jobs](../interactive/job_control.md#suspending-foreground-jobs).)

On error, it returns a non-zero exit status.

## Examples

See [Job control](../interactive/job_control.md).

## Compatibility

Many implementations allow omitting the leading `%` from job IDs and
specifying multiple job IDs at once, though this is not required by POSIX.

POSIX does not require the `fg` built-in to conform to the [Utility Syntax Guidelines](https://pubs.opengroup.org/onlinepubs/9799919799/basedefs/V1_chap12.html#tag_12_02), which means portable scripts cannot use any options or the `--` separator for the built-in.
