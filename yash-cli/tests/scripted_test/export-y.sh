# export-y.sh: yash-specific test of the export built-in

test_O -d -e n 'export rejects non-portable variable name' -o portable
export foo-bar=1
echo not reached
__IN__

test_O -d -e n 'export rejects variable name starting with a digit' -o portable
export 1abc=1
echo not reached
__IN__

test_OE -e 0 'export accepts non-portable variable name without the portable option'
export foo-bar=1
__IN__

test_O -d -e n 'export rejects missing operand' -o portable
export
echo not reached
__IN__

test_O -d -e n 'export rejects operand with the -p option' -o portable
export foo=bar
export -p foo
echo not reached
__IN__

test_oE 'export error message mentions the portable option' -o portable
(export) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_oE 'export error message names the unexpected operand' -o portable
export foo=bar
(export -p foo) 2>result
grep -Fq foo result && echo shown
__IN__
shown
__OUT__

test_oE -e 0 'export accepts the -p option without operands' -o portable
export foo=bar
export -p | grep -Fx 'export foo=bar'
__IN__
export foo=bar
__OUT__

test_oE -e 0 'export accepts operands without the -p option' -o portable
export foo=bar
echo "$foo"
__IN__
bar
__OUT__

test_oE -e 0 'export accepts missing operand without the portable option'
export foo=bar
export | grep -Fx 'export foo=bar'
__IN__
export foo=bar
__OUT__

test_oE -e 0 'export accepts operand with the -p option without the portable option'
export foo=bar
export -p foo
__IN__
export foo=bar
__OUT__
