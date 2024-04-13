# bg-p.sh: test of the bg built-in for any POSIX-compliant shell

posix="true"

# The "sleep 0" commands in the test cases below are a hack to ensure that the
# shell receives SIGCHLD and takes in the latest status of the background jobs.
# Without this, the "wait" built-in may return before the background jobs are
# actually resumed.

cat >job1 <<\__END__
exec sh -c 'kill -s STOP $$; echo'
__END__

chmod a+x job1
ln job1 job2

test_O -d -e n 'bg cannot be used when job control is disabled' +m
:&
bg
__IN__

test_oE 'default operand chooses most recently suspended job' -m
:&
sh -c 'kill -s STOP $$; echo 1'
bg >/dev/null
sleep 0
wait
__IN__
1
__OUT__

test_OE 'already running job is ignored' -m
while kill -s CONT $$; do sleep 1; done &
bg >/dev/null
kill %
__IN__

test_OE -e 17 'resumed job is awaitable' -m
sh -c 'kill -s STOP $$; exit 17'
bg >/dev/null
sleep 0
wait %
__IN__

test_O -e n 'resumed job is in background' -m
sh -c 'kill -s STOP $$; trap "" TTIN; head -n 1 /dev/tty'
# The shell is ignoring SIGTTIN, so the "head" command will just fail with EIO
# when it tries to read from the terminal in the background.
bg >/dev/null
sleep 0
wait %
__IN__

test_oE 'specifying job ID' -m
./job1
./job2
echo -
bg %./job1 >/dev/null
bg %./job2 >/dev/null
sleep 0
wait
__IN__
-


__OUT__

test_oE 'specifying more than one job ID' -m
./job1
./job2
echo -
bg %./job1 %./job2 >/dev/null
sleep 0
wait
__IN__
-


__OUT__

test_OE -e 0 'bg prints resumed job' -m
sleep 1&
bg >bg.out
grep -qx '\[[[:digit:]][[:digit:]]*][[:blank:]]*sleep 1' bg.out
__IN__

test_OE -e 0 'exit status of bg' -m
sh -c 'kill -s STOP $$; exit 17'
bg >/dev/null
__IN__

test_O -d -e n 'no existing job' -m
bg
__IN__

test_O -d -e n 'no such job' -m
sh -c 'kill -s STOP $$'
bg %_no_such_job_
exit_status=$?
fg >/dev/null
exit $exit_status
__IN__
