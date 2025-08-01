# Fg built-in

The **`fg`** resumes a suspended job in the foreground.

## Synopsis

```sh
fg [job_id]
```

## Description

The built-in brings the specified job to the foreground and resumes its
execution by sending the `SIGCONT` signal to it. The built-in then waits for
the job to finish (or suspend again).

If the resumed job finishes, it is removed from the [job list](JobList).
If the job gets suspended again, it is set as the [current
job](JobList::current_job).

## Options

None.

## Operands

Operand *job_id* specifies which job to resume. See the module documentation
of [`yash_env::job::id`] for the format of job IDs. If omitted, the built-in
resumes the [current job](JobList::current_job).

(TODO: allow omitting the leading `%`)
(TODO: allow multiple operands)

## Standard output

The built-in writes the selected job name to the standard output.

(TODO: print the job number as well)

## Errors

This built-in can be used only when job control is enabled.

The built-in fails if the specified job is not found, not job-controlled, or
not [owned] by the current shell environment.

## Exit status

If a resumed job suspends and the current environment is
[interactive](Env::is_interactive), the built-in returns with the
[`Interrupt`] divert, which should make the shell stop the current command
execution and return to the prompt. Otherwise, the built-in returns with the
exit status of the resumed job.

On error, it returns a non-zero exit status.

## Portability

Many implementations allow omitting the leading `%` from job IDs and
specifying multiple job IDs at once, though this is not required by POSIX.
