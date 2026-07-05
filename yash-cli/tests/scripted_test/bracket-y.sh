# bracket-y.sh: yash-specific test of double-bracket reserved words

test_O -d -e 2 'the ]] reserved word cannot be a command name'
]]
__IN__
