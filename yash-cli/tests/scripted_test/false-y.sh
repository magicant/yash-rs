# false-y.sh: yash-specific test of the false built-in

test_OE -e n 'false ignores arguments'
false foo -x --
__IN__

test_OE -e n 'false without arguments is silent under the portable option' -o portable
false
__IN__

test_O -d -e n 'false warns about an operand under the portable option' -o portable
false foo
__IN__

test_O -d -e n 'false warns about an option-like argument under the portable option' -o portable
false -x
__IN__

test_O -d -e n 'false warns about -- under the portable option' -o portable
false --
__IN__

test_oE 'false argument warning mentions the portable option' -o portable
(false foo) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__
