# trap2-y.sh: yash-specific test for signal trapping

test_o 'EXIT trap in job-controlled pipeline' -m
: | trap 'echo exited 1' EXIT &
wait
trap 'echo exited 2' EXIT | cat &
wait
__IN__
exited 1
exited 2
__OUT__

test_o 'EXIT trap in command substitution'
foo=$(trap 'echo exited' EXIT)
echo "$foo"
__IN__
exited
__OUT__
