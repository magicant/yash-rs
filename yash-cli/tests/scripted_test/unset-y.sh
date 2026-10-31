# unset-y.sh: yash-specific test of the unset built-in

test_OE -e 0 'long option name accepted as an extension'
foo=1
unset --variables foo
__IN__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
unset --variables foo
__IN__

test_OE -e 0 'short option name still accepted under the portable option' -o portable
foo=1
unset -v foo
__IN__

test_O -d -e n 'unset rejects missing operand under the portable option' -o portable
unset
echo not reached
__IN__

test_O -d -e n 'unset -v rejects missing operand under the portable option' -o portable
unset -v
echo not reached
__IN__

test_O -d -e n 'unset -f rejects missing operand under the portable option' -o portable
unset -f
echo not reached
__IN__

test_oE 'unset missing operand error message mentions the portable option' -o portable
(unset) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_OE -e 0 'unset accepts an operand under the portable option' -o portable
foo=1
unset foo
__IN__

test_OE -e 0 'unset accepts missing operand without the portable option'
unset
__IN__

test_oE -e 0 'help option prints help without exiting the shell'
unset --help
echo $?
__IN__
unset: unset variables or functions

Usage:
  unset [-f|-v] NAME...

Options:
  -f, --functions  unset functions
  -v, --variables  unset variables (default)
      --help       print this help

See <https://magicant.github.io/yash-rs/builtins/unset.html> for details.
0
__OUT__

test_O -d -e 2 'help option with another operand'
unset --help foo
__IN__
