# alias-y.sh: yash-specific test of aliases

test_O 'portable option ignores aliases with non-portable names' -o portable
alias no.such.utility='echo substituted'
no.such.utility
__IN__

test_oE 'alias built-in errors on non-portable alias names' -o portable
alias a.b='echo substituted' 2>result
echo $?
grep -Fq 'not portable' result && echo errored
grep -Fq 'a.b' result && echo name_shown
alias a.b >/dev/null 2>&1 || echo not_defined
__IN__
1
errored
name_shown
not_defined
__OUT__

test_oE 'portable option expands aliases with portable names' -o portable
alias a-b='echo substituted'
a-b
__IN__
substituted
__OUT__
