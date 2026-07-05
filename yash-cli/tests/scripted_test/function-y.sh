# function-y.sh: yash-specific test of function definition

# POSIX requires a function name to be an unquoted NAME consisting solely of
# underscores, digits, and alphabetics from the portable character set, not
# starting with a digit. The portable option rejects other forms.

test_O -d -e 2 'portable option rejects a name starting with a digit' -o portable
1a() { :; }
__IN__

test_O -d -e 2 'portable option rejects a quoted name' -o portable
'a'() { :; }
__IN__

test_O -d -e 2 'portable option rejects a name with an expansion' -o portable
a=a
$a() { :; }
__IN__

test_oE 'portable option allows a portable name' -o portable
_Az9() { echo foo; }
_Az9
__IN__
foo
__OUT__

test_OE -e 0 'without portable, a name starting with a digit is accepted'
1a() { :; }
__IN__
