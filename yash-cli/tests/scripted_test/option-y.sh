# option-y.sh: yash-specific test of shell options

test_oE 'allexport in many contexts' -a
unset a b
: $((a=1)) ${b=2}
sh -c 'echo ${a-unset} ${b-unset}'
__IN__
1 2
__OUT__

test_x -e 0 'hashondef (long) on: $-' -o hashondef
printf '%s\n' "$-" | grep -q h
__IN__

test_x -e 0 'hashondef (long) off: $-' +o hashondef
printf '%s\n' "$-" | grep -qv h
__IN__

test_O 'noexec takes effect immediately'
set -n; echo not executed
__IN__

test_o 'noexec is ineffective when interactive' -in +m --norcfile
echo printed; exit; echo not printed
__IN__
printed
__OUT__

: TODO not yet implemented <<\__OUT__
test_OE -e 0 'pipefail on: single command successful pipe' --pipefail
true
__IN__

: TODO not yet implemented <<\__OUT__
test_OE -e 13 'pipefail on: single command unsuccessful pipe' --pipefail
(exit 13)
__IN__

: TODO not yet implemented <<\__OUT__
test_OE -e 0 'pipefail on: multi-command successful pipe' --pipefail
true | true | true | true
__IN__

: TODO not yet implemented <<\__OUT__
test_OE -e 7 'pipefail on: multi-command unsuccessful pipe' --pipefail
true | exit 2 | true | exit 7 | true | true
__IN__

: TODO not yet implemented <<\__OUT__
test_OE -e 7 'pipefail on: multi-command unsuccessful pipe in subshell' \
    --pipefail
(true | exit 2 | true | exit 7 | true | true)
__IN__

: TODO not yet implemented <<\__OUT__
test_oE 'traceall on: effect' --traceall
exec 2>&1
COMMAND_NOT_FOUND_HANDLER='echo not found $* >&2; HANDLED=1'
set -xv
no/such/command
__IN__
no/such/command
+ no/such/command
+ echo not found no/such/command
not found no/such/command
+ HANDLED=1
__OUT__

: TODO not yet implemented <<\__OUT__
test_oE 'traceall on: effect' --notraceall
exec 2>&1
COMMAND_NOT_FOUND_HANDLER='echo not found $* >&2; HANDLED=1'
set -xv
no/such/command
__IN__
no/such/command
+ no/such/command
not found no/such/command
__OUT__

test_O -d -e 2 'unset off: unset variable $((foo))' -u
eval '$((x))'
__IN__

: TODO not yet implemented <<\__ERR__
test_oe 'xtrace on: recursion' -x
PS4='$(echo X)+ '
echo 1
echo 2
__IN__
1
2
__OUT__
X+ PS4='$(echo X)+ '
X+ echo 1
X+ echo 2
__ERR__

test_x -e 0 'abbreviation of -o argument' -o allex
echo $- | grep -q a
__IN__

test_x -e 0 'abbreviation of +o argument' -a +o allexport
echo $- | grep -qv a
__IN__

test_x -e 0 'concatenation of -o and argument' -oallexport
echo $- | grep -q a
__IN__

test_x -e 0 'concatenation of option and -o' -ao errexit
echo $- | grep a | grep -q e
__IN__

test_x -e 0 'concatenation of option and -o and argument' -aoerrexit
echo $- | grep a | grep -q e
__IN__
