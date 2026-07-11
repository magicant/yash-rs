# alias-y.sh: yash-specific test of aliases

test_oE 'portable option ignores aliases with non-portable names' -o portable
alias a.b='echo substituted'
a.b 2>/dev/null || echo ignored
__IN__
ignored
__OUT__

test_oE 'portable option expands aliases with portable names' -o portable
alias a-b='echo substituted'
a-b
__IN__
substituted
__OUT__
