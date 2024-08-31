# job-y.sh: yash-specific test of job control

test_e 'interactive shell reports job status before prompt' -im
echo >&2; sleep 0& while kill -0 $! 2>/dev/null; do :; done
echo done >&2; exit
__IN__
$ 
[1] + Done                 sleep 0
$ done
__ERR__

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
