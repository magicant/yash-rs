# echo-y.sh: yash-specific test of the echo built-in

test_oE 'echo interprets escape sequences by default'
echo 'a\tb\\c' | tr '\t' T
echo 'a\nb'
echo '\0101\0102'
__IN__
aTb\c
a
b
AB
__OUT__

test_oE 'echo stops printing at \c'
echo 'foo\cbar' baz
echo
__IN__
foo
__OUT__

test_oE 'echo treats -n as an operand by default'
echo -n foo
__IN__
-n foo
__OUT__

test_oE 'echo keeps unknown escape sequences'
echo '\q\'
__IN__
\q\
__OUT__

test_oE 'ECHO_STYLE=SYSV'
ECHO_STYLE=SYSV echo -n 'a\0102'
__IN__
-n aB
__OUT__

test_oE 'ECHO_STYLE=XSI'
ECHO_STYLE=XSI echo -n 'a\0102'
__IN__
-n aB
__OUT__

test_oE 'ECHO_STYLE=BSD'
ECHO_STYLE=BSD echo -n 'a\0102'
echo
ECHO_STYLE=BSD echo -e foo
__IN__
a\0102
-e foo
__OUT__

test_oE 'ECHO_STYLE=GNU'
ECHO_STYLE=GNU echo 'a\0102'
ECHO_STYLE=GNU echo -e 'a\0102'
ECHO_STYLE=GNU echo -n -E 'a\0102'
echo
__IN__
a\0102
aB
a\0102
__OUT__

test_oE 'ECHO_STYLE=ZSH'
ECHO_STYLE=ZSH echo 'a\0102'
ECHO_STYLE=ZSH echo -E 'a\0102'
ECHO_STYLE=ZSH echo -ne 'a\0102'
echo
__IN__
aB
a\0102
aB
__OUT__

test_oE 'ECHO_STYLE=DASH'
ECHO_STYLE=DASH echo -n 'a\0102'
echo
ECHO_STYLE=DASH echo -e foo
__IN__
aB
-e foo
__OUT__

test_oE 'ECHO_STYLE=RAW'
ECHO_STYLE=RAW echo -n 'a\0102'
__IN__
-n a\0102
__OUT__

test_oE 'ECHO_STYLE is case-insensitive and only its first letter matters'
ECHO_STYLE=bogus echo -n foo
echo
__IN__
foo
__OUT__

test_oE 'echo with unknown ECHO_STYLE behaves as SYSV'
ECHO_STYLE=unknown echo -n 'a\0102'
__IN__
-n aB
__OUT__

test_oE 'echo stops parsing options at a non-option'
ECHO_STYLE=GNU echo -n -x -n
echo
__IN__
-x -n
__OUT__

test_oE -e 0 'echo with portable arguments under the portable option' -o portable
echo foo -- -x
__IN__
foo -- -x
__OUT__

test_O -d -e 2 'echo rejects an option-like first operand under the portable option' -o portable
echo -nE foo
__IN__

test_O -d -e 2 'echo rejects a backslash under the portable option' -o portable
ECHO_STYLE=RAW echo foo 'a\b'
__IN__

test_oE 'echo portability error mentions the portable option' -o portable
(echo 'a\b') 2>result
grep -Fq portable result && echo shown
__IN__
shown
__OUT__

test_oE 'posixlycorrect does not disable ECHO_STYLE' -o posixlycorrect
ECHO_STYLE=BSD echo -n foo
echo
__IN__
foo
__OUT__

test_O -d -e 1 'echo fails when the standard output is closed'
echo foo >&-
__IN__
