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

test_OE -e 0 'jobs accepts -l and -p together without the portable option'
jobs -l -p
__IN__

test_O -d -e 2 'jobs rejects -l and -p together under the portable option' -o portable
jobs -l -p
__IN__

test_O -d -e 2 'jobs rejects -p and -l together under the portable option' -o portable
jobs -p -l
__IN__

test_O -d -e 2 'jobs rejects -lp in one argument under the portable option' -o portable
jobs -lp
__IN__

test_oE 'jobs conflicting option error message mentions the portable option' -o portable
(jobs -l -p) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_OE -e 0 'jobs accepts a repeated -l under the portable option' -o portable
jobs -l -l
__IN__
