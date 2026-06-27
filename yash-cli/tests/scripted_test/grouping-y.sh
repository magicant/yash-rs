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
