# cd-y.sh: yash-specific test of the cd built-in

cd -P .
export ORIGPWD="$PWD"
mkdir dir -

test_O -d -e 3 'unset HOME'
unset HOME
cd
__IN__

test_O -d -e 3 'empty HOME'
HOME=
cd
__IN__

testcase "$LINENO" 'unset PWD' \
    3<<\__IN__ 5</dev/null 4<<__OUT__
unset CDPATH PWD
cd dir
echo ---
printf 'PWD=%s\nOLDPWD=%s\n' "$PWD" "$OLDPWD"
pwd
__IN__
---
PWD=dir
OLDPWD=
$ORIGPWD/dir
__OUT__

test_O -d -e 3 'unset OLDPWD'
unset OLDPWD
cd -
__IN__

# It is POSIXly unclear what the exit status of cd should be in this case.
testcase "$LINENO" -d 'read-only PWD' 3<<\__IN__ 4<<__OUT__
unset CDPATH
readonly PWD
cd dir
echo --- $?
printf 'PWD=%s\n' "$PWD"
pwd
__IN__
--- 0
PWD=$ORIGPWD
$ORIGPWD/dir
__OUT__

# It is POSIXly unclear what the exit status of cd should be in this case.
test_o -d 'unset OLDPWD'
unset CDPATH
readonly OLDPWD=/
cd dir
echo --- $?
printf 'OLDPWD=%s\n' "$OLDPWD"
__IN__
--- 0
OLDPWD=/
__OUT__

: TODO not implemented yet <<\__OUT__
test_oE -e 0 'YASH_AFTER_CD is iteratively executed after changing directory'
YASH_AFTER_CD=(
'printf "PWD=%s\n" "$PWD"'
'printf "status=%d\n" "$?"'
'(exit 1); break -i'
'echo not reached')
(exit 11)
cd /
__IN__
PWD=/
status=11
__OUT__

(
posix="true"

test_OE -e 0 'YASH_AFTER_CD is ignored (-o POSIX)'
YASH_AFTER_CD='echo not reached'
cd /
__IN__

)

(
if ! [ / -ef /.. ]; then
    skip="true"
fi

: TODO not implemented yet <<\__OUT__
test_oE '/.. is canonicalized to / (+o POSIX)'
cd /..//../dev
printf '%s\n' "$PWD"
cd /../..
printf '%s\n' "$PWD"
__IN__
/dev
/
__OUT__

(
posix="true"

test_oE '/.. is kept intact (-o POSIX)'
cd /../../dev
printf '%s\n' "$PWD"
__IN__
/../../dev
__OUT__

)

)

testcase "$LINENO" 'redundant slashes are removed' \
    3<<\__IN__ 5</dev/null 4<<__OUT__
cd .//dir///
pwd
__IN__
$ORIGPWD/dir
__OUT__

: TODO not implemented yet <<\__OUT__
test_oE 'default directory option with operand'
HOME=/tmp cd --default-directory=/ /dev
echo --- $?
pwd
__IN__
--- 0
/dev
__OUT__

: TODO not implemented yet <<\__OUT__
test_oE 'default directory option without operand'
HOME=/tmp cd --default-directory=/
echo --- $?
pwd
__IN__
--- 0
/
__OUT__

: TODO not implemented yet <<\__OUT__
testcase "$LINENO" 'hyphen is literal in default directory option' \
    3<<\__IN__ 5</dev/null 4<<__OUT__
OLDPWD=/ cd --default-directory=-
pwd
__IN__
$ORIGPWD/-
__OUT__

test_O -d -e 4 'too many operands'
cd . .
__IN__

test_O -d -e 4 'invalid option'
cd --no-such-option
__IN__

test_O -e 0 'printing to closed stream'
OLDPWD=/ cd - >&-
__IN__

# vim: set ft=sh ts=8 sts=4 sw=4 et:
