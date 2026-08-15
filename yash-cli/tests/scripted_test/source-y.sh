# source-y.sh: yash-specific test of the dot built-in

test_oE -e 0 'source works as an alias for the dot built-in'
source /dev/null
echo reached
__IN__
reached
__OUT__

test_O -d -e 126 'source is rejected as a non-portable alias under the portable option' -o portable
source /dev/null
__IN__

test_oE -e 0 'the dot built-in is not rejected under the portable option' -o portable
. /dev/null
echo reached
__IN__
reached
__OUT__

test_oE 'error message for source (rejected) names the portable option' -o portable
source /dev/null 2>result
grep -Fq 'portable' result && echo shown
__IN__
shown
__OUT__

test_O -d -e 2 'the dot built-in rejects a surplus operand under the portable option' -o portable
. /dev/null foo
__IN__

test_oE 'dot surplus operand error message names the operand' -o portable
(. /dev/null foo) 2>result
grep -Fq foo result && echo shown
__IN__
shown
__OUT__

test_oE 'dot surplus operand error message mentions the portable option' -o portable
(. /dev/null foo) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_oE -e 0 'the dot built-in accepts a surplus operand without the portable option'
. /dev/null foo
echo reached
__IN__
reached
__OUT__
