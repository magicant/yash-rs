# exit-y.sh: yash-specific test of the exit built-in

test_OE -e 3 'long option name accepted as an extension'
exit --force 3
__IN__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
exit --force 3
__IN__

test_OE -e 3 'short option name still accepted under the portable option' -o portable
exit -f 3
__IN__
