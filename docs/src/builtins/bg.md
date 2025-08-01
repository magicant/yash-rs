# Bg built-in

This module implements the [`bg` built-in], which resumes suspended jobs in
the background.

## Implementation notes

This implementation sends the `SIGCONT` signal even to jobs that are already
running. The signal is not sent to jobs that have already terminated, to
prevent unrelated processes that happen to have the same process IDs as the
jobs from receiving the signal.

The built-in sets the [expected state] of the resumed jobs to
[`ProcessState::Running`] so that the status changes are not reported again
on the next command prompt.

[`bg` built-in]: https://magicant.github.io/yash-rs/builtins/bg.html
[expected state]: yash_env::job::Job::expected_state