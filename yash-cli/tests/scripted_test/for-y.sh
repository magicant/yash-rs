# for-y.sh: yash-specific test of for loop

# POSIX requires the for-loop variable name to be an unquoted NAME consisting
# solely of underscores, digits, and alphabetics from the portable character
# set, not starting with a digit. The portable option rejects other forms.

test_O -d -e 2 'portable option rejects a name starting with a digit' -o portable
for 1a do :; done
__IN__

test_O -d -e 2 'portable option rejects a quoted name' -o portable
for 'A' do :; done
__IN__

test_O -d -e 2 'portable option rejects a name with an expansion' -o portable
for $A do :; done
__IN__

test_oE 'portable option allows a portable name' -o portable
for _Az9 in value; do echo "$_Az9"; done
__IN__
value
__OUT__

test_OE -e 0 'without portable, a name starting with a digit is accepted'
for 1a in value; do :; done
__IN__
