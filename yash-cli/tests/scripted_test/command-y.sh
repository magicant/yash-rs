# command-y.sh: yash-specific test of the command built-in

# Seemingly meaningless comments like #` in this script are to work around
# syntax highlighting errors on some editors.

# TODO Option not yet implemented
test_oE -e 0 -f 'executing with -b option'
command -b eval echo foo
__IN__
foo
__OUT__

# TODO Option not yet implemented
test_O -d -e 127 -f 'external command is not found with -b option'
command -b cat /dev/null
__IN__

# TODO Option not yet implemented
test_OE -e 0 -f 'executing with -e option'
command -e cat /dev/null
__IN__

# TODO Option not yet implemented
test_O -d -e 127 -f 'built-in command is not found with -e option'
PATH=
command -e exit 10
__IN__

# TODO Option not yet implemented
test_oE -e 0 -f 'executing with -f option'
exit() { echo foo; }
command -f exit 1
__IN__
foo
__OUT__

# TODO function keyword not yet implemented
test_oE -e 0 -f 'executing function with name containing slash'
function foo/bar {
    echo "$@"
}
command -f foo/bar baz 'x  x'
__IN__
baz x  x
__OUT__

# TODO Option not yet implemented
test_O -d -e 127 -f 'external command is not found with -f option'
command -f cat /dev/null
__IN__

test_O -d -e 127 'executing substitutive built-in missing in $PATH'
PATH=
command false
__IN__

test_oE 'error message for substitutive built-in missing in $PATH names $PATH'
(PATH=; command false) 2>result
grep -Fq '$PATH' result && echo shown
__IN__
shown
__OUT__

test_O -d -e 126 'executing elective built-in under the portable option' -o portable
command typeset
__IN__

test_OE -e 1 'describing elective built-in under the portable option (-v)' -o portable
command -v typeset
__IN__

test_O -d -e 1 'describing elective built-in under the portable option (-V)' -o portable
command -V typeset
__IN__

test_oE 'error message for elective built-in (-V) names the portable option' -o portable
command -V typeset 2>result
grep -Fq 'portable' result && echo shown
__IN__
shown
__OUT__

test_O -d -e 126 'executing source (non-portable alias for the dot built-in) under the portable option' -o portable
command source /dev/null
__IN__

test_OE -e 1 'describing source under the portable option (-v)' -o portable
command -v source
__IN__

test_O -d -e 1 'describing source under the portable option (-V)' -o portable
command -V source
__IN__

test_oE -e 0 'describing alias (-V)'
alias a='foo'
command -V a
__IN__
a: alias for `foo`
__OUT__
#`

test_oE -e 0 'describing special built-ins (-V)'
command -V : . break continue eval exec exit export readonly return set shift \
    times trap unset
__IN__
:: special built-in
.: special built-in
break: special built-in
continue: special built-in
eval: special built-in
exec: special built-in
exit: special built-in
export: special built-in
readonly: special built-in
return: special built-in
set: special built-in
shift: special built-in
times: special built-in
trap: special built-in
unset: special built-in
__OUT__

test_oE -e 0 'describing mandatory built-ins (-V)'
command -V alias bg cd command fg getopts jobs kill read \
    type umask unalias wait
__IN__
alias: mandatory built-in
bg: mandatory built-in
cd: mandatory built-in
command: mandatory built-in
fg: mandatory built-in
getopts: mandatory built-in
jobs: mandatory built-in
kill: mandatory built-in
read: mandatory built-in
type: mandatory built-in
umask: mandatory built-in
unalias: mandatory built-in
wait: mandatory built-in
__OUT__

# TODO hash built-in is not yet implemented
# TODO merge with the above test
test_oE -e 0 -f 'describing mandatory built-in hash (-V)'
command -V hash
__IN__
hash: mandatory built-in
__OUT__

test_oE -e 0 'describing mandatory built-in ulimit (-V)'
command -V ulimit
__IN__
ulimit: mandatory built-in
__OUT__

# TODO array built-in is not yet implemented
test_oE -e 0 -f 'describing extension built-in (-V)'
command -V array
__IN__
array: extension built-in
__OUT__

# TODO echo built-in is not yet implemented
test_OE -f 'describing substitutive built-in echo (-V)'
command -V echo | grep -v "^echo: substitutive built-in "
__IN__

test_OE 'describing substitutive built-in false (-V)'
command -V false | grep -v "^false: substitutive built-in "
__IN__

test_OE 'describing substitutive built-in true (-V)'
command -V true | grep -v "^true: substitutive built-in "
__IN__

test_OE 'describing substitutive built-in pwd (-V)'
command -V pwd | grep -v "^pwd: substitutive built-in "
__IN__

test_OE 'describing external command (-V)'
command -V cat | grep -v '^cat: external utility at '
__IN__

test_oE -e 0 'describing function (-V)'
true() { :; }
command -V true
__IN__
true: function
__OUT__

test_oE -e 0 'describing reserved words (-V)'
command -V if then else elif fi do done case esac while until for function \
    { } ! in [[ ]] select namespace
__IN__
if: keyword
then: keyword
else: keyword
elif: keyword
fi: keyword
do: keyword
done: keyword
case: keyword
esac: keyword
while: keyword
until: keyword
for: keyword
function: keyword
{: keyword
}: keyword
!: keyword
in: keyword
[[: keyword
]]: keyword
select: keyword
namespace: keyword
__OUT__

# TODO Option not yet implemented
test_oE -e 0 -f 'describing alias with -a option'
alias a='foo'
command -va a &&
command --identify --alias a
__IN__
alias a=foo
alias a=foo
__OUT__

# TODO Option not yet implemented
test_oE -e 0 -f 'describing built-ins with -b option'
command -vb : bg &&
command --identify --builtin-command : bg
__IN__
:
bg
:
bg
__OUT__

# TODO Option not yet implemented
test_E -e 0 -f 'describing external command with -e option'
command -ve cat &&
command --identify --external-command cat
__IN__

(
cd -P . # normalize $PWD
case $PWD in (//*|*/) skip="true"; esac

>foo
chmod a+x foo

testcase "$LINENO" \
    -e 0 'output of describing absolute external command (-v, with slash)' \
    3<<'__IN__' 4<<__OUT__ 5</dev/null
command -v "${PWD}/foo"
__IN__
${PWD}/foo
__OUT__

testcase "$LINENO" \
    -e 0 'output of describing relative external command (-v, with slash)' -e \
    3<<'__IN__' 4<<__OUT__ 5</dev/null
command -v "./foo"
cd /
command -v "${OLDPWD#/}/foo"
__IN__
${PWD}/./foo
${PWD}/foo
__OUT__

)

# TODO Option not yet implemented
test_oE -e 0 -f 'describing function with -f option'
true() { :; }
command -vf true &&
command --identify --function true
__IN__
true
true
__OUT__

# TODO Option not yet implemented
test_oE -e 0 -f 'describing reserved word with -k option'
command -vk if &&
command --identify --keyword if
__IN__
if
if
__OUT__

# TODO Option not yet implemented
test_OE -e 1 -f 'describing non-existent command (-va)'
command -va exit
__IN__

# TODO Option not yet implemented
test_OE -e 1 -f 'describing non-existent command (-vb)'
command -vb cat
__IN__

# TODO Option not yet implemented
test_OE -e 1 -f 'describing non-existent command (-ve)'
PATH=
command -ve exit
__IN__

# TODO Option not yet implemented
test_OE -e 1 -f 'describing non-existent command (-vk)'
command -vk exit
__IN__

# TODO Option not yet implemented
test_OE -e 1 -f 'describing non-existent command (-vf)'
command -vf exit
__IN__

test_O -d -e 1 'describing non-existent command (-V)'
PATH=
command -V _no_such_command_
__IN__

test_oE -e 0 'describing with long option'
command --verbose-identify if : bg
__IN__
if: keyword
:: special built-in
bg: mandatory built-in
__OUT__

test_O -d -e 1 'printing to closed stream'
command -v command >&-
__IN__

test_O -d -e n 'using -a without -v'
command -a :
__IN__

test_O -d -e n 'using -k without -v'
command -k :
__IN__

test_O -d -e n 'invalid option'
command --no-such-option
__IN__

test_OE -e 0 'long option name accepted as an extension'
command --path :
__IN__

test_O -d -e n 'long option name rejected under the portable option' -o portable
command --path :
__IN__

test_OE -e 0 'short option name still accepted under the portable option' -o portable
command -p :
__IN__

test_OE -e 0 'missing operand (non-POSIX)'
command
__IN__

test_O -d -e 2 'command rejects missing operand under the portable option' -o portable
command
__IN__

test_O -d -e 2 'command -v rejects missing operand under the portable option' -o portable
command -v
__IN__

test_O -d -e 2 'command -V rejects missing operand under the portable option' -o portable
command -V
__IN__

test_oE 'command missing operand error message mentions the portable option' -o portable
(command) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_oE -e 0 'command accepts an operand under the portable option' -o portable
command echo foo
__IN__
foo
__OUT__

test_O -d -e 2 'command -v rejects more than one operand under the portable option' -o portable
command -v cat cat
__IN__

test_O -d -e 2 'command -V rejects more than one operand under the portable option' -o portable
command -V cat cat
__IN__

test_oE 'command surplus operand error message names the operand' -o portable
(command -v cat dog) 2>result
grep -Fq dog result && echo shown
__IN__
shown
__OUT__

test_oE 'command surplus operand error message mentions the portable option' -o portable
(command -v cat dog) 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_oE -e 0 'command -v accepts one operand under the portable option' -o portable
command -v :
__IN__
:
__OUT__

test_oE -e 0 'command accepts arguments after the command name under the portable option' -o portable
command echo foo bar
__IN__
foo bar
__OUT__

test_oE -e 0 'command -v accepts more than one operand without the portable option'
command -v : bg
__IN__
:
bg
__OUT__

(
posix="true"

test_o -d 'argument syntax error in special built-in does not kill shell'
command . # missing operand
echo reached
__IN__
reached
__OUT__

)
