# pwd-y.sh: yash-specific test of the pwd built-in

test_OE -e 0 'long option name accepted as an extension'
pwd --physical >/dev/null
__IN__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
pwd --physical >/dev/null
__IN__

test_OE -e 0 'short option name still accepted under the portable option' -o portable
pwd -P >/dev/null
__IN__
