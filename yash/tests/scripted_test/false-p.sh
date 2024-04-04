# false-p.sh: test of the false built-in for any POSIX-compliant shell

posix="true"

test_OE -e n 'false'
PATH=
false
__IN__
