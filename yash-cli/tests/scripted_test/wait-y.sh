# wait-y.sh: yash-specific test of the wait built-in

test_O -d -e 2 'lone % rejected under the portable option' -o portable
wait %
__IN__

test_O -d -e 1 'ambiguous job ID reported as a runtime failure' -m
sleep 10 & sleep 11 &
wait %sleep
__IN__
