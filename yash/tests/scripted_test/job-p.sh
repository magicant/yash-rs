# job-p.sh: test of job control for any POSIX-compliant shell

posix="true"

mkfifo sync

test_x -e 17 'job result is not lost when reported automatically (+b)' -im
exec >sync && exit 17 &
pid=$!
cat sync
:
:
:
wait $pid
__IN__

# This test is in async-p.tst.
#test_oE 'stdin of asynchronous list is null without job control' +m

test_oE 'stdin of asynchronous list is not modified with job control' -m
tail -n 1& wait
echo this line should be skipped by tail
echo this line should be printed by tail
__IN__
echo this line should be printed by tail
__OUT__
