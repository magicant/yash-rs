# set-y.sh: yash-specific test of the set built-in

test_O -d -e 2 'set built-in rejects long option under the portable option' -o portable
set --errexit
__IN__

test_O -d -e 2 'set built-in rejects ++ long option under the portable option' -o portable
set ++errexit
__IN__

test_O -d -e 2 'set built-in rejects -o name POSIX spells negatively' -o portable
set -o clobber
__IN__

test_O -d -e 2 'set built-in rejects abbreviated -o name under the portable option' -o portable
set -o errex
__IN__

test_O -d -e 2 'set built-in rejects -o name in wrong case under the portable option' -o portable
set -o ERREXIT
__IN__

test_O -d -e 2 'set built-in rejects -o argument attached to the option name' -o portable
set -oerrexit
__IN__

test_O -d -e 2 'set built-in rejects -o name POSIX does not specify' -o portable
set -o posixlycorrect
__IN__

test_O -d -e 2 'set built-in rejects short option POSIX does not specify' -o portable
set -l
__IN__

test_O -d -e 2 'set built-in rejects - as a separator under the portable option' -o portable
set - foo
__IN__

test_O -d -e 2 'set built-in rejects - alone under the portable option' -o portable
set -
__IN__

test_O -d -e 2 'set built-in rejects - after an option under the portable option' -o portable
set -a - foo
__IN__

test_x -e 0 'set built-in accepts long option without the portable option'
set --errexit
echo "$-" | grep -q e
__IN__

test_x -e 0 'set built-in accepts ++ long option without the portable option'
set -e
set ++errexit
echo "$-" | grep -qv e
__IN__

test_oE -e 0 'set built-in accepts -o clobber without the portable option'
set -o clobber
set +o | grep clobber
__IN__
set -o clobber
__OUT__

test_x -e 0 'set built-in accepts abbreviated -o name without the portable option'
set -o errex
echo "$-" | grep -q e
__IN__

test_x -e 0 'set built-in accepts -o name in wrong case without the portable option'
set -o ERREXIT
echo "$-" | grep -q e
__IN__

test_x -e 0 'set built-in accepts -o argument attached to the option name without the portable option'
set -oerrexit
echo "$-" | grep -q e
__IN__

test_oE -e 0 'set built-in accepts non-POSIX -o name without the portable option'
set -o posixlycorrect
set +o | grep posixlycorrect
__IN__
set -o posixlycorrect
__OUT__

test_x -e 0 'set built-in accepts non-POSIX short option without the portable option'
set -l
echo "$-" | grep -q l
__IN__

test_x -e 0 'set built-in accepts POSIX short options under the portable option' -o portable
set -e -C +C -x +x
echo "$-" | grep -q e
__IN__

test_x -e 0 'set built-in accepts POSIX -o names under the portable option' -o portable
set -o noclobber -o errexit +o noclobber
echo "$-" | grep -q e
__IN__

test_o 'set built-in accepts - as a separator without the portable option'
set - foo
echo "$@"
__IN__
foo
__OUT__

test_o 'set built-in accepts -- as a separator under the portable option' -o portable
set -- - foo
echo "$@"
__IN__
- foo
__OUT__

test_o 'set built-in accepts - as an operand under the portable option' -o portable
set -- foo -
echo "$@"
__IN__
foo -
__OUT__

test_x -e 0 'set built-in can turn off the portable option' -o portable
set +o portable
set --errexit
echo "$-" | grep -q e
__IN__

test_x -e 0 'enabling the portable option affects only following options'
set --errexit -o portable
echo "$-" | grep -q e
__IN__

test_O -d -e 2 'option after enabling the portable option is checked'
set -o portable --xtrace
__IN__

test_OE -e 0 'set +o output restores options under the portable option' -o portable
set -o > expected
saveset=$(set +o)
set +o portable
set -a -o posixlycorrect
eval "$saveset"
set -o | diff expected -
__IN__

test_OE -e 0 'set +o output restores options without the portable option'
set -o login -o ignoreeof
set -o > expected
saveset=$(set +o)
set +o login +o ignoreeof -o posixlycorrect
eval "$saveset"
set -o | diff expected -
__IN__

test_OE -e 0 'set +o output is executable while the portable option is on'
saveset=$(set +o)
set -o portable
eval "$saveset"
__IN__
