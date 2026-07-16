# readonly-y.sh: yash-specific test of the readonly built-in

test_O -d -e n 'making PWD OLDPWD OPTIND OPTARG LINENO read-only is rejected' -o portable
readonly PWD OLDPWD OPTIND OPTARG LINENO
echo not reached
__IN__

test_O -d -e n 'making PWD OLDPWD OPTIND OPTARG LINENO read-only with values is rejected' -o portable
readonly PWD=/tmp OLDPWD=/tmp OPTIND=1 OPTARG=x LINENO=1
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

test_oE 'readonly can make PWD read-only without the portable option'
readonly PWD
echo ok
__IN__
ok
__OUT__
