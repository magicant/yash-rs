# alias-y.sh: yash-specific test of aliases

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

test_OE -e 0 'unalias long option name accepted as an extension'
alias a='echo a'
unalias --all
alias
__IN__

test_O -d -e 2 'unalias long option name rejected under the portable option' -o portable
unalias --all
__IN__

test_OE -e 0 'unalias short option name still accepted under the portable option' -o portable
unalias -a
__IN__
