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
    { } ! in
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

test_OE -e 0 'missing operand (non-POSIX)'
command
__IN__

(
posix="true"

test_o -d 'argument syntax error in special built-in does not kill shell'
command . # missing operand
echo reached
__IN__
reached
__OUT__

# TODO Should error out because of the missing operand
test_O -d -e n -f 'missing operand (w/o -v, POSIX)'
command
__IN__

# TODO Should error out because of the missing operand
test_O -d -e n -f 'missing operand (with -v, POSIX)'
command -v
__IN__

# TODO Should error out because of the missing operand
test_O -d -e n -f 'missing operand (with -V, POSIX)'
command -V
__IN__

# TODO Should error out because of too many operands
test_O -d -e n -f 'more than one operand (with -v, POSIX)'
command -v foo bar
__IN__

test_O -d -e n 'more than one operand (with -V, POSIX)'
command -V foo bar
__IN__

)
