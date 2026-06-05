# intr-y.sh: yash-specific test suite for interrupt handling

test_o -e 0 'interrupting a command in interactive mode' -i --norcfile
kill -INT $$; echo 'This should not be printed.'
kill -l $?
__IN__
INT
__OUT__
