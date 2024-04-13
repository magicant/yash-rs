# fg-p.sh: test of the fg built-in for any POSIX-compliant shell

posix="true"

cat >job1 <<\__END__
exec sh -c 'echo 1; kill -s STOP $$; echo 2'
__END__

cat >job2 <<\__END__
exec sh -c 'echo a; kill -s STOP $$; echo b'
__END__

chmod a+x job1
chmod a+x job2

mkfifo fifo

test_O -d -e n 'fg cannot be used when job control is disabled' +m
:&
fg
__IN__

test_oE 'default operand chooses most recently suspended job' -m
:&
sh -c 'kill -s STOP $$; echo 1'
fg >/dev/null
__IN__
1
__OUT__

: TODO Needs to find a way to test this <<\__IN__
test_oE 'resumed job is in foreground' -m
sh -c 'kill -s STOP $$; ...'
fg >/dev/null
__IN__

test_x -e 127 'resumed job is disowned unless suspended again' -m
cat fifo >/dev/null &
exec 3>fifo
kill -s STOP %
exec 3>&-
fg >/dev/null
wait $!
__IN__

test_oE 'specifying job ID' -m
./job1
./job2
fg %./job1 >/dev/null
fg %./job2 >/dev/null
__IN__
1
a
2
b
__OUT__

test_oE 'fg prints resumed job' -m
./job1
fg
__IN__
1
./job1
2
__OUT__

test_O -d -e n 'no existing job' -m
fg
__IN__

test_O -d -e n 'no such job' -m
sh -c 'kill -s STOP $$'
fg %_no_such_job_
exit_status=$?
fg >/dev/null
exit $exit_status
__IN__
