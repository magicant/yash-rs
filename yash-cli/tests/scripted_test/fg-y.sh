# fg-y.sh: yash-specific test of the fg built-in

test_O -d -e 2 'lone % rejected as an operand under the portable option' -m -o portable
sh -c 'kill -s STOP $$'
fg %
__IN__

test_O -d -e 2 'operand without the leading % rejected as a syntax error' -m
sh -c 'kill -s STOP $$'
fg 1
__IN__
