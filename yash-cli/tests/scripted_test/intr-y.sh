# intr-y.sh: yash-specific test suite for interrupt handling

test_o -e 0 'interrupting the shell in interactive mode' -im --norcfile
kill -INT $$; echo 'This should not be printed.'
kill -l $?
__IN__
INT
__OUT__

test_o -e 0 'interrupting an external utility in interactive mode' -im --norcfile
sh -c 'kill -INT $$'; echo 'This should not be printed.'
kill -l $?
__IN__
INT
__OUT__

test_o -e 0 'interrupting a subshell in interactive mode' -im --norcfile
(kill -INT 0); echo 'This should not be printed.'
kill -l $?
__IN__
INT
__OUT__

test_o -e 0 'interrupting a multi-command pipeline in interactive mode' -im --norcfile
sleep 1 | kill -INT 0; echo 'This should not be printed.'
kill -l $?
__IN__
INT
__OUT__

test_o -e 0 'interrupting a redirection in interactive mode' -im --norcfile
> foo$(kill -INT 0); echo 'This should not be printed.'
kill -l $?
test -e foo || echo 'The file was not created.'
__IN__
INT
The file was not created.
__OUT__

test_o -e 0 'interrupting a command substitution in interactive mode' -im --norcfile
echo $(sh -c 'kill -INT $$'); echo 'This should not be printed.'
kill -l $?
__IN__
INT
__OUT__

# There is no test for interrupting the wait built-in because there is no
# reliable way to send SIGINT to the shell after the wait built-in has started
# waiting but before it finishes. The unit test in yash-builtin covers this.
