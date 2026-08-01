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
