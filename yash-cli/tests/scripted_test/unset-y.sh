# unset-y.sh: yash-specific test of the unset built-in

test_OE -e 0 'long option name accepted as an extension'
foo=1
unset --variables foo
__IN__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
unset --variables foo
__IN__

test_OE -e 0 'short option name still accepted under the portable option' -o portable
foo=1
unset -v foo
__IN__
