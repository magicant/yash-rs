# arith-y.sh: yash-specific tests of arithmetic expansion

# POSIX does not require the increment and decrement operators. The portable
# option rejects them in arithmetic expansion.

test_O -d -e 2 'portable option rejects prefix increment' -o portable
echo $((++foo))
__IN__

test_O -d -e 2 'portable option rejects postfix decrement' -o portable
echo $((foo--))
__IN__

test_oE 'without portable, increment and decrement operators are accepted'
foo=1
echo $((++foo)) $((foo--)) "$foo"
__IN__
2 2 1
__OUT__
