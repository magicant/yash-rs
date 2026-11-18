# echo-p.sh: test of the echo built-in for any POSIX-compliant shell

posix="true"

test_oE -e 0 'echo without operands prints a newline'
echo
__IN__

__OUT__

test_oE -e 0 'echo prints operands separated by spaces'
echo foo  bar '' baz
__IN__
foo bar  baz
__OUT__

test_oE -e 0 'echo prints -- as an operand'
echo -- foo
__IN__
-- foo
__OUT__

test_oE -e 0 'echo prints - as an operand'
echo - foo
__IN__
- foo
__OUT__
