# grouping-y.sh: yash-specific test of grouping commands

# POSIX recognizes `}` as a reserved word only at the start of a command or
# after another reserved word. The portable option therefore rejects a `}` that
# follows a subshell (which ends with `)`) or a redirection without a separator,
# but accepts a `}` that follows another reserved word (such as the inner `}`,
# `fi`, or `done`).

test_O -d -e 2 'portable option rejects } after a subshell' -o portable
{ ( : ) }
__IN__

test_OE -e 0 'without portable, } after a subshell is accepted'
{ ( : ) }
__IN__

test_O -d -e 2 'portable option rejects } after a redirected grouping' -o portable
{ { :; } >/dev/null }
__IN__

test_OE -e 0 'without portable, } after a redirected grouping is accepted'
{ { :; } >/dev/null }
__IN__

test_oE 'portable option allows } with a separator before it' -o portable
{ ( : ); }
{ { :; } >/dev/null; }
echo ok
__IN__
ok
__OUT__

test_oE 'portable option allows } after a reserved word' -o portable
{ { echo a; } }
{ if true; then echo b; fi }
{ for x in c; do echo $x; done }
__IN__
a
b
c
__OUT__

# Other shells parse `((` as an arithmetic command, so the portable option
# rejects it at the beginning of a command. A space (`( (`) is portable.

test_O -d -e 2 'portable option rejects (( at the beginning of a command' -o portable
((echo hello))
__IN__

test_oE 'without portable, (( is parsed as nested subshells'
((echo hello))
__IN__
hello
__OUT__

test_oE 'portable option allows ( ( with a space' -o portable
( (echo hello))
__IN__
hello
__OUT__

# Other shells parse `!(` as an extended glob, so the portable option rejects
# it at the beginning of a command. A space (`! (`) is portable.

test_O -d -e 2 'portable option rejects !( at the beginning of a command' -o portable
!(false)
__IN__

test_OE -e 0 'without portable, !( is parsed as a negated subshell'
!(false)
__IN__

test_OE -e 0 'portable option allows ! ( with a space' -o portable
! (false)
__IN__
