# kill-y.sh: yash-specific test of the kill built-in

test_OE -e 0 'option -n accepted without the portable option'
kill -n CONT $$
__IN__

test_O -d -e 2 'option -n rejected under the portable option' -o portable
kill -n CONT $$
__IN__

test_OE -e 0 'option -v accepted without the portable option'
kill -v >/dev/null
__IN__

test_O -d -e 2 'option -v rejected under the portable option' -o portable
kill -v
__IN__

test_OE -e 0 'attached signal argument accepted without the portable option'
kill -sCONT $$
__IN__

test_O -d -e 2 'attached signal argument rejected under the portable option' -o portable
kill -sCONT $$
__IN__

test_OE -e 0 'signal number argument to -s accepted without the portable option'
sleep 10 &
kill -s 9 $!
wait $!
:
__IN__

test_O -d -e 2 'signal number argument to -s rejected under the portable option' -o portable
kill -s 9 $$
__IN__

test_oE -e 0 'multiple -l operands accepted without the portable option'
kill -l 9 15
__IN__
KILL
TERM
__OUT__

test_O -d -e 2 'multiple -l operands rejected under the portable option' -o portable
kill -l 9 15
__IN__

test_oE -e 0 'signal name operand to -l accepted without the portable option'
kill -l TERM
__IN__
TERM
__OUT__

test_O -d -e 2 'signal name operand to -l rejected under the portable option' -o portable
kill -l TERM
__IN__

test_OE -e 0 'separate signal name argument accepted under the portable option' -o portable
kill -s CONT $$
__IN__

test_OE -e 0 'null signal accepted under the portable option' -o portable
kill -s 0 $$
__IN__

test_OE -e 0 'obsolete signal name syntax accepted under the portable option' -o portable
kill -CONT $$
__IN__

test_OE -e 0 'obsolete signal number syntax accepted under the portable option' -o portable
kill -0 $$
__IN__

test_OE -e 0 'bare signal name starting with s accepted under the portable option' -o portable
sleep 10 &
kill -stop $!
kill -cont $!
kill -s KILL $!
wait $!
:
__IN__

test_oE -e 0 'single numeric operand to -l accepted under the portable option' -o portable
kill -l 15
__IN__
TERM
__OUT__

test_OE -e 0 'option -l without operands accepted under the portable option' -o portable
kill -l >/dev/null
__IN__
