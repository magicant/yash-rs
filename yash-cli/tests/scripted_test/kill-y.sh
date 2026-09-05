# kill-y.sh: yash-specific test of the kill built-in

test_OE -e 0 'SIG prefix accepted in the argument to -s'
kill -s SIGCONT $$
kill -s sigcont $$
__IN__

test_O -d -e 2 'SIG prefix in the argument to -s rejected under the portable option' -o portable
kill -s SIGCONT $$
__IN__

test_OE -e 0 'SIG prefix accepted in the obsolete syntax'
kill -SIGCONT $$
kill -sigcont $$
__IN__

test_O -d -e 2 'SIG prefix in the obsolete syntax rejected under the portable option' -o portable
kill -SIGCONT $$
__IN__

test_OE -e 0 'option -n accepted without the portable option'
kill -n CONT $$
__IN__

test_O -d -e 2 'option -n rejected under the portable option' -o portable
kill -n CONT $$
__IN__

test_OE -e 0 'option -v accepted without the portable option'
kill -v >/dev/null
__IN__

test_O -d -e 2 'option -v rejected under the portable option' -o portable
kill -v
__IN__

test_OE -e 0 'attached signal argument accepted without the portable option'
kill -sCONT $$
__IN__

test_O -d -e 2 'attached signal argument rejected under the portable option' -o portable
kill -sCONT $$
__IN__

test_OE -e 0 'signal number argument to -s accepted without the portable option'
sleep 10 &
kill -s 9 $!
wait $!
:
__IN__

test_O -d -e 2 'signal number argument to -s rejected under the portable option' -o portable
kill -s 9 $$
__IN__

test_oE -e 0 'multiple -l operands accepted without the portable option'
kill -l 9 15
__IN__
KILL
TERM
__OUT__

test_O -d -e 2 'multiple -l operands rejected under the portable option' -o portable
kill -l 9 15
__IN__

test_oE -e 0 'signal name operand to -l accepted without the portable option'
kill -l TERM
__IN__
TERM
__OUT__

test_O -d -e 2 'signal name operand to -l rejected under the portable option' -o portable
kill -l TERM
__IN__

test_OE -e 0 'separate signal name argument accepted under the portable option' -o portable
kill -s CONT $$
__IN__

test_OE -e 0 'null signal accepted under the portable option' -o portable
kill -s 0 $$
__IN__

test_OE -e 0 'obsolete signal name syntax accepted under the portable option' -o portable
kill -CONT $$
__IN__

test_OE -e 0 'obsolete signal number syntax accepted under the portable option' -o portable
kill -0 $$
__IN__

test_OE -e 0 'bare signal name starting with s accepted under the portable option' -o portable
sleep 10 &
kill -stop $!
kill -cont $!
kill -s KILL $!
wait $!
:
__IN__

test_oE -e 0 'single numeric operand to -l accepted under the portable option' -o portable
kill -l 15
__IN__
TERM
__OUT__

test_OE -e 0 'option -l without operands accepted under the portable option' -o portable
kill -l >/dev/null
__IN__

test_O -d -e 2 'lone % rejected as a target under the portable option' -o portable
kill -s CONT %
__IN__

test_O -d -e 2 'target that is neither a job ID nor a process ID rejected as a syntax error'
kill -s CONT foo
__IN__

(
# The state keyword of ps is not specified by POSIX. Skip the test case where
# it is not supported rather than reporting a failure that is not the shell's.
[ "$(ps -o state= -p $$ 2>/dev/null)" ] || skip="true"

# The job below is suspended, so its process must still be in the stopped
# state ("T") after kill has rejected the targets. Had the signal been sent,
# the process would have been killed and left as a zombie. The test case
# resumes the job at the end so that the process does not linger.
test_o -d -e 0 'kill sends no signal when a later target has a syntax error' -m
sh -c 'kill -s STOP $$'
kill -s KILL %1 foo
echo "kill: $?"
# The unquoted expansion drops the padding some ps implementations add.
state=$(echo $(ps -o state= -p "$(jobs -p %1)"))
case $state in
    T*) echo "job is stopped";;
    *) echo "job state is $state";;
esac
kill -s CONT %1
wait %1
__IN__
kill: 2
job is stopped
__OUT__
)
