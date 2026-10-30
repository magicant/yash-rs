# wait-y.sh: yash-specific test of the wait built-in

test_O -d -e 2 'lone % rejected under the portable option' -o portable
wait %
__IN__

test_O -d -e 1 'ambiguous job ID reported as a runtime failure' -m
trap 'kill -s KILL %1 %2' EXIT
sleep 10 & sleep 11 &
wait %sleep
__IN__

test_oE -e 0 'help option prints help'
wait --help
__IN__
wait: wait for jobs to finish

Usage:
  wait [JOB_ID_OR_PROCESS_ID...]

Options:
      --help  print this help

See <https://magicant.github.io/yash-rs/builtins/wait.html> for details.
__OUT__

test_O -d -e 2 'help option with another operand'
wait --help %1
__IN__
