# job-y.sh: yash-specific test of job control

user_id="$(id -u)"

# This test case first creates a background job that immediately exits, then
# waits for the job to finish, sending a null signal to the job to poll if the
# job is still running. A subshell starts another job and waits for it to finish
# to make sure the main shell process receives the SIGCHLD signal and examines
# the latest job status. The test case checks if the job is reported as done
# before the prompt for the next line is displayed.
(
if [ "$user_id" -eq 0 ]; then
    skip="true"
fi

test_e 'interactive shell reports job status before prompt (non-root)' -im
echo >&2; sleep 0& while kill -0 $! 2>/dev/null; do :; done; (sleep 0& wait)
echo done >&2; exit
__IN__
$ 
[1] + Done                 sleep 0
$ done
__ERR__
)

(
if [ "$user_id" -ne 0 ]; then
    skip="true"
fi

test_e 'interactive shell reports job status before prompt (root)' -im
echo >&2; sleep 0& while kill -0 $! 2>/dev/null; do :; done; (sleep 0& wait)
echo done >&2; exit
__IN__
# 
[1] + Done                 sleep 0
# done
__ERR__
)

mkfifo sync

# According to POSIX, a shell may, but is not required to, forget the job
# when the -b option is on. Yash forgets it.
test_x -e 17 'job result is not lost when reported automatically (-b)' -bim
exec >sync && exit 17 &
pid=$!
cat sync
:
:
:
wait $pid
__IN__

# This is a POSIX requirement, but this test case depends on the shell's
# behavior that runs all pipeline components in child processes.
test_o -e 0 'discard remaining commands when a command suspends' -im
echo resumed | { kill -STOP 0; cat; }; echo not printed 1; echo not printed 2&
fg >/dev/null
__IN__
resumed
__OUT__
