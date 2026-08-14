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
