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

# The readonly built-in is a special built-in, so its error would terminate
# the shell. The command built-in is used to keep the shell running and
# observe what the rejected readonly built-in did.

test_oE 'rejected readonly still assigns the value to PWD' -o portable
command readonly PWD=/somewhere 2>/dev/null
echo "status=$?"
echo "PWD=$PWD"
__IN__
status=1
PWD=/somewhere
__OUT__

test_oE 'rejected readonly leaves PWD writable' -o portable
command readonly PWD=/somewhere 2>/dev/null
PWD=/elsewhere
echo "PWD=$PWD"
__IN__
PWD=/elsewhere
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

test_O -d -e n 'readonly rejects missing operand' -o portable
readonly
echo not reached
__IN__

test_O -d -e n 'readonly rejects operand with the -p option' -o portable
readonly foo=bar
readonly -p foo
echo not reached
__IN__

test_oE 'readonly error message mentions the portable option' -o portable
(readonly) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_oE 'readonly error message names the unexpected operand' -o portable
readonly foo=bar
(readonly -p foo) 2>result
grep -Fq foo result && echo shown
__IN__
shown
__OUT__

test_oE -e 0 'readonly accepts the -p option without operands' -o portable
readonly foo=bar
readonly -p | grep -Fx 'readonly foo=bar'
__IN__
readonly foo=bar
__OUT__

test_oE -e 0 'readonly accepts operands without the -p option' -o portable
readonly foo=bar
echo "$foo"
__IN__
bar
__OUT__

test_oE -e 0 'readonly accepts missing operand without the portable option'
readonly foo=bar
readonly | grep -Fx 'readonly foo=bar'
__IN__
readonly foo=bar
__OUT__

test_oE -e 0 'readonly accepts operand with the -p option without the portable option'
readonly foo=bar
readonly -p foo
__IN__
readonly foo=bar
__OUT__

test_O -d -e n 'readonly rejects long option name' -o portable
readonly --print
echo not reached
__IN__

test_O -d -e n 'readonly rejects abbreviated long option name' -o portable
readonly --p
echo not reached
__IN__

test_oE 'readonly long option error message mentions the portable option' -o portable
(readonly --print) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_oE -e 0 'readonly accepts long option name without the portable option'
readonly foo=bar
readonly --print foo
__IN__
readonly foo=bar
__OUT__

test_oE 'readonly ++print error message does not blame the portable option' -o portable
(readonly ++print) 2>result
grep -Fq portable result || echo not shown
__IN__
not shown
__OUT__
