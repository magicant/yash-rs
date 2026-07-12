# simple-y.sh: yash-specific test of simple commands

# POSIX reserves words whose final character is a `:` for possible future use,
# so a command name ending with a `:` produces unspecified results. The portable
# option therefore rejects such a command name, except the lone `:` (colon)
# built-in.

test_O -d -e 2 'portable option rejects a command name ending with a colon' -o portable
foo:
__IN__

test_O -d -e 2 'portable option rejects a colon-suffixed command name after a reserved word' -o portable
if foo:; then true; fi
__IN__

# Any words ending with a `:` are reserved, even if they may contain a parameter expansion.
test_O -d -e 2 'portable option rejects a command name ending with a colon (2)' -o portable
$foo:
__IN__

test_oE 'portable option allows a colon at the end of an argument' -o portable
echo foo:
__IN__
foo:
__OUT__

test_OE -e 0 'portable option allows the lone colon built-in' -o portable
:
__IN__

# Technically, an assignment whose value ends with a `:` is reserved, but it
# would be inconvenient to reject such assignments. The portable option
# therefore allows them.
test_oE 'portable option allows an assignment value ending with a colon' -o portable
v=foo:
echo "$v"
__IN__
foo:
__OUT__

test_O -d -e 127 'without portable, a command name ending with a colon is parsed'
foo:
__IN__

# A portable name consists solely of underscores, digits, and alphabetics from
# the portable character set, not starting with a digit. The portable option
# rejects assignment names that do not meet this form, since other
# POSIX-conforming shells may not support them.

test_O -d -e 2 'portable option rejects an assignment name starting with a digit' -o portable
1a=foo
__IN__

test_O -d -e 2 'portable option rejects an assignment name with a non-portable character' -o portable
a.b=foo
__IN__

test_oE 'portable option allows a portable assignment name' -o portable
_Az9=foo
echo "$_Az9"
__IN__
foo
__OUT__

test_OE -e 0 'without portable, an assignment name starting with a digit is accepted'
1a=foo
__IN__

# Arrays are a non-POSIX extension, so the portable option rejects array
# assignments.

test_O -d -e 2 'portable option rejects an array assignment' -o portable
a=(b c)
__IN__

test_O -d -e 2 'portable option rejects an empty array assignment' -o portable
a=()
__IN__

test_oE 'without portable, an array assignment is accepted'
a=(b c)
for i in "$a"; do echo "$i"; done
__IN__
b
c
__OUT__
