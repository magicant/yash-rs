# jobs-y.sh: yash-specific test of the jobs built-in

test_OE -e 0 'long option name accepted as an extension'
jobs --verbose
__IN__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
jobs --verbose
__IN__

test_OE -e 0 'short option name still accepted under the portable option' -o portable
jobs -l
__IN__

test_O -d -e 2 'jobs rejects -lp in one argument'
jobs -lp
__IN__

test_OE -e 0 'jobs accepts a repeated -l option'
jobs -l -l
__IN__

test_O -d -e 2 'lone % rejected under the portable option' -o portable
jobs %
__IN__

test_O -d -e 2 'operand without the leading % rejected under the portable option' -o portable
jobs 1
__IN__
