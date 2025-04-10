# type-y.sh: yash-specific test of the type built-in

test_oE -e 0 'describing alias'
alias a='foo'
type a
__IN__
a: alias for `foo`
__OUT__

test_oE -e 0 'describing special built-ins'
type : . break continue eval exec exit export readonly return set shift \
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

test_oE -e 0 'describing mandatory built-ins'
type alias bg cd command fg getopts jobs kill read \
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
test_oE -e 0 -f 'describing mandatory built-in hash'
type hash
__IN__
hash: mandatory built-in
__OUT__

test_oE -e 0 'describing mandatory built-in ulimit'
type ulimit
__IN__
ulimit: mandatory built-in
__OUT__

# TODO array built-in is not yet implemented
test_oE -e 0 -f 'describing extension built-in'
type array
__IN__
array: extension built-in
__OUT__

# TODO echo built-in is not yet implemented
test_OE -f 'describing substitutive built-in echo'
type echo | grep -v "^echo: substitutive built-in "
__IN__

test_OE 'describing substitutive built-in false'
type false | grep -v "^false: substitutive built-in "
__IN__

test_OE 'describing substitutive built-in true'
type true | grep -v "^true: substitutive built-in "
__IN__

test_OE 'describing substitutive built-in pwd'
type pwd | grep -v "^pwd: substitutive built-in "
__IN__

test_OE 'describing external command'
type cat | grep -v '^cat: external utility at '
__IN__

test_oE -e 0 'describing function'
true() { :; }
type true
__IN__
true: function
__OUT__

test_oE -e 0 'describing reserved words'
type if then else elif fi do done case esac while until for function \
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
type -a a &&
type --alias a
__IN__
a: alias for `foo`
a: alias for `foo`
__OUT__

# TODO Option not yet implemented
test_oE -e 0 -f 'describing built-ins with -b option'
type -b : bg &&
type --builtin-command : bg
__IN__
:: special built-in
bg: mandatory built-in
:: special built-in
bg: mandatory built-in
__OUT__

# TODO Option not yet implemented
test_E -e 0 -f 'describing external command with -e option'
type -e cat &&
type --external-command cat
__IN__

(
cd -P . # normalize $PWD
case $PWD in (//*|*/) skip="true"; esac

>foo
chmod a+x foo

testcase "$LINENO" \
    -e 0 'output of describing absolute external command (with slash)' \
    3<<'__IN__' 4<<__OUT__ 5</dev/null
type "${PWD}/foo"
__IN__
${PWD}/foo: external utility at ${PWD}/foo
__OUT__

testcase "$LINENO" \
    -e 0 'output of describing relative external command (with slash)' -e \
    3<<'__IN__' 4<<__OUT__ 5</dev/null
type "./foo"
cd /
type "${OLDPWD%/}/foo"
__IN__
./foo: external utility at ${PWD%/}/./foo
${PWD%/}/foo: external utility at ${PWD%/}/foo
__OUT__

)

# TODO Option not yet implemented
test_oE -e 0 -f 'describing function with -f option'
true() { :; }
type -f true &&
type --function true
__IN__
true: function
true: function
__OUT__

# TODO Option not yet implemented
test_oE -e 0 -f 'describing reserved word with -k option'
type -k if &&
type --keyword if
__IN__
if: keyword
if: keyword
__OUT__

# TODO Option not yet implemented
test_OE -e 1 -f 'describing non-existent command (-a)'
type -a exit
__IN__

# TODO Option not yet implemented
test_OE -e 1 -f 'describing non-existent command (-b)'
type -b cat
__IN__

# TODO Option not yet implemented
test_OE -e 1 -f 'describing non-existent command (-e)'
PATH=
type -e exit
__IN__

# TODO Option not yet implemented
test_OE -e 1 -f 'describing non-existent command (-k)'
type -k exit
__IN__

# TODO Option not yet implemented
test_OE -e 1 -f 'describing non-existent command (-f)'
type -f exit
__IN__

test_O -d -e 1 'printing to closed stream'
type command >&-
__IN__

test_O -d -e n 'invalid option'
type --no-such-option
__IN__

test_OE -e 0 'missing operand (non-POSIX)'
type
__IN__

(
posix="true"

# TODO Should error out because of the missing operand
test_O -d -e 2 -f 'missing operand (POSIX)'
type
__IN__

# TODO Should error out because of too many operands
test_O -d -e 2 -f 'more than one operand (POSIX)'
type foo bar
__IN__

)
