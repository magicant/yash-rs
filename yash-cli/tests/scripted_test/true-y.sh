# true-y.sh: yash-specific test of the true built-in

test_OE -e 0 'true ignores arguments'
true foo -x --
__IN__

test_OE -e 0 'true without arguments is silent under the portable option' -o portable
true
__IN__

test_O -d -e 0 'true warns about an operand under the portable option' -o portable
true foo
__IN__

test_O -d -e 0 'true warns about an option-like argument under the portable option' -o portable
true -x
__IN__

test_O -d -e 0 'true warns about -- under the portable option' -o portable
true --
__IN__

test_oE 'true argument warning mentions the portable option' -o portable
(true foo) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__
