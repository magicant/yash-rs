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
