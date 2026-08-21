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

test_OE -e 0 'long option name accepted as an extension'
ulimit --hard >/dev/null
__IN__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
ulimit --hard
__IN__

test_OE -e 0 'short option name still accepted under the portable option' -o portable
ulimit -H >/dev/null
__IN__

test_O -d -e 2 'resource option -b (sbsize) rejected under the portable option' -o portable
ulimit -b
__IN__

test_O -d -e 2 'resource option -e (nice) rejected under the portable option' -o portable
ulimit -e
__IN__

test_O -d -e 2 'resource option -i (sigpending) rejected under the portable option' -o portable
ulimit -i
__IN__

test_O -d -e 2 'resource option -k (kqueues) rejected under the portable option' -o portable
ulimit -k
__IN__

test_O -d -e 2 'resource option -l (memlock) rejected under the portable option' -o portable
ulimit -l
__IN__

test_O -d -e 2 'resource option -m (rss) rejected under the portable option' -o portable
ulimit -m
__IN__

test_O -d -e 2 'resource option -q (msgqueue) rejected under the portable option' -o portable
ulimit -q
__IN__

test_O -d -e 2 'resource option -r (rtprio) rejected under the portable option' -o portable
ulimit -r
__IN__

test_O -d -e 2 'resource option -R (rttime) rejected under the portable option' -o portable
ulimit -R
__IN__

test_O -d -e 2 'resource option -u (nproc) rejected under the portable option' -o portable
ulimit -u
__IN__

test_O -d -e 2 'resource option -w (swap) rejected under the portable option' -o portable
ulimit -w
__IN__

test_O -d -e 2 'resource option -x (locks) rejected under the portable option' -o portable
ulimit -x
__IN__

test_O -d -e 2 'specifying -a and -f at once'
ulimit -a -f
__IN__

test_OE -e 0 'grouped option letters accepted as an extension'
ulimit -Sf >/dev/null
__IN__

test_O -d -e 2 'grouped option letters rejected under the portable option' -o portable
ulimit -Sf
__IN__

test_OE -e 0 'separate option letters still accepted under the portable option' -o portable
ulimit -S -f >/dev/null
__IN__

test_O -d -e 2 'giving -H and -S together rejected under the portable option' -o portable
ulimit -H -S 0
__IN__

test_OE -e 0 'repeated -H still accepted under the portable option' -o portable
ulimit -H -H >/dev/null
__IN__

test_O -d -e 2 'repeated resource option rejected under the portable option' -o portable
ulimit -f -f
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
