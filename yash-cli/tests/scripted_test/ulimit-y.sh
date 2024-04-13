# ulimit-y.sh: yash-specific test of the ulimit built-in

test_O -d -e 2 'too many operands (w/o -a)'
ulimit 0 0
__IN__

test_O -d -e 2 'too many operands (with -a)'
ulimit -a 0
__IN__

test_O -d -e 2 'invalid option --xxx'
ulimit --no-such=option
__IN__

test_O -d -e 2 'specifying -a and -f at once'
ulimit -a -f
__IN__

test_O -d -e 2 'invalid operand (non-numeric)'
ulimit X
__IN__

test_O -d -e 2 'invalid operand (non-integral)'
ulimit 1.0
__IN__

test_O -d -e 2 'invalid operand (negative)'
ulimit -- -1
__IN__

test_O -d -e 1 'printing to closed output stream'
ulimit >&-
__IN__
