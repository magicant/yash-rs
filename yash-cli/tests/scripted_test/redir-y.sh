# redir-y.sh: yash-specific test of redirections

test_OE -e 0 'without portable, an IO_NUMBER operand is accepted'
> 1
: < 1>/dev/null
__IN__

test_O -d -e 2 'an IO_LOCATION prefix is not supported'
{n}>/dev/null
__IN__

# The portable option rejects non-portable redirection operators and operands.

test_O -d -e 2 'portable option rejects an IO_LOCATION prefix' -o portable
{n}>/dev/null
__IN__

test_O -d -e 2 'portable option rejects the >>| operator' -o portable
echo not reached >>| /dev/null
__IN__

test_O -d -e 2 'portable option rejects the <<< operator' -o portable
cat <<< not_reached
__IN__

test_O -d -e 2 'portable option rejects an IO_NUMBER as operand' -o portable
> 1
: < 1>/dev/null
__IN__

test_O -d -e 2 'portable option rejects an IO_LOCATION as operand' -o portable
: < {n}>/dev/null
__IN__

test_oE 'portable option allows the portable redirection operators' -o portable
echo hi >| f
cat f
echo bye >> f
cat f
__IN__
hi
hi
bye
__OUT__
