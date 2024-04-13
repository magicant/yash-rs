# nop-p.sh: test of the colon, true, and false built-ins

posix="true"

test_OE -e 0 'colon (no arguments)'
:
__IN__

test_OE -e 0 'colon (some arguments)'
: unused ignored arguments
__IN__

test_OE -e 0 'true'
true
__IN__

test_OE -e n 'false'
false
__IN__
