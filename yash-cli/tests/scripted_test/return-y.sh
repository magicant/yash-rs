# return-y.sh: yash-specific test of the return built-in

test_OE -e 3 'long option name accepted as an extension'
f() { return --no-return 3; }
f
__IN__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
f() { return --no-return 3; }
f
__IN__

test_O -d -e 2 'short option name also rejected under the portable option' -o portable
f() { return -n 3; }
f
__IN__
