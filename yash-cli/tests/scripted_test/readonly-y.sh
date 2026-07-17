# readonly-y.sh: yash-specific test of the readonly built-in

test_O -d -e n 'making PWD read-only is rejected' -o portable
readonly PWD
echo not reached
__IN__

test_O -d -e n 'making OLDPWD read-only is rejected' -o portable
readonly OLDPWD
echo not reached
__IN__

test_O -d -e n 'making OPTIND read-only is rejected' -o portable
readonly OPTIND
echo not reached
__IN__

test_O -d -e n 'making OPTARG read-only is rejected' -o portable
readonly OPTARG
echo not reached
__IN__

test_O -d -e n 'making LINENO read-only is rejected' -o portable
readonly LINENO
echo not reached
__IN__

test_O -d -e n 'making PWD read-only with a value is rejected' -o portable
readonly PWD=/tmp
echo not reached
__IN__

test_oE 'readonly error message names the rejected variable' -o portable
(readonly PWD) 2>result
grep -Fq 'PWD' result && echo shown
__IN__
shown
__OUT__

test_oE 'typeset -r rejects PWD under the portable option' -o portable
typeset -r PWD 2>/dev/null
echo $?
__IN__
1
__OUT__

test_oE 'value is still assigned when making PWD read-only fails' -o portable
typeset -r PWD=/somewhere 2>/dev/null
echo "$PWD"
__IN__
/somewhere
__OUT__

test_OE -e 0 'readonly can make PWD read-only without the portable option'
readonly PWD
__IN__

test_O -d -e n 'readonly rejects non-portable variable name' -o portable
readonly foo-bar
echo not reached
__IN__

test_O -d -e n 'readonly rejects non-portable variable name with a value' -o portable
readonly foo-bar=1
echo not reached
__IN__

test_oE 'readonly error message names the non-portable variable name' -o portable
(readonly foo-bar) 2>result
grep -Fq 'foo-bar' result && echo shown
__IN__
shown
__OUT__

test_OE -e 0 'readonly accepts non-portable variable name without the portable option'
readonly foo-bar=1
__IN__
