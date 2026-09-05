# bg-y.sh: yash-specific test of the bg built-in

test_O -d -e 2 'lone % rejected as an operand under the portable option' -m -o portable
sh -c 'kill -s STOP $$'
bg %
__IN__

test_O -d -e 2 'operand without the leading % rejected as a syntax error' -m
sh -c 'kill -s STOP $$'
bg 1
__IN__

(
# The state keyword of ps is not specified by POSIX. Skip the test case where
# it is not supported rather than reporting a failure that is not the shell's.
[ "$(ps -o state= -p $$ 2>/dev/null)" ] || skip="true"

# The job below is suspended, so its process must still be in the stopped
# state ("T") after bg has rejected the operands. The test case resumes the
# job at the end so that the process does not linger.
test_o -d -e 0 'bg resumes no job when a later operand has a syntax error' -m
sh -c 'kill -s STOP $$'
bg %1 1
echo "bg: $?"
# The unquoted expansion drops the padding some ps implementations add.
state=$(echo $(ps -o state= -p "$(jobs -p %1)"))
case $state in
    T*) echo "job is stopped";;
    *) echo "job state is $state";;
esac
kill -s CONT %1
wait %1
__IN__
bg: 2
job is stopped
__OUT__
)
