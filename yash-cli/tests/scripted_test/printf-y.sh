# printf-y.sh: yash-specific test of the printf built-in

test_oE -e 0 'printf interprets extended escape sequences in format'
printf '\"\047\e[\x41\x4a1あ\U0001F600\n' | od -An -c | tr -s ' '
__IN__
 " ' 033 [ A J 1 343 201 202 360 237 230 200 \n
__OUT__

test_oE -e 0 'printf prints unknown escape sequences as they are'
printf '\q\x\c\n'
__IN__
\q\x\c
__OUT__

test_oE -e 0 'printf %f, %F'
printf '%f|%.2f|%.0f|%#.0f|%+015f|%-10.1F|\n' 1.25 3.14159 2.5 1 -1000 1.5
printf '%f|%F|%F|%5f\n' inf -infinity nan -inf
__IN__
1.250000|3.14|2|1.|-0001000.000000|1.5       |
inf|-INF|NAN| -inf
__OUT__

test_oE -e 0 'printf %e, %E'
printf '%e|%.3E|%.0e|%#.0e|%+15e\n' 1234.5 0.000025 1 1 -1
__IN__
1.234500e+03|2.500E-05|1e+00|1.e+00|  -1.000000e+00
__OUT__

test_oE -e 0 'printf %g, %G'
printf '%g|%g|%g|%g|%G\n' 0 1.25 -1000 1234567 0.000025
printf '%.3g|%#.3g|%#g|%.0g\n' -1000 1 0.0001 15
__IN__
0|1.25|-1000|1.23457e+06|2.5E-05
-1e+03|1.00|0.000100000|2e+01
__OUT__

test_oE -e 0 'printf with hexadecimal floating-point arguments'
printf '%g|%g\n' 0x1p4 0x.8
__IN__
16|0.5
__OUT__

test_oE -e 0 'printf %q'
printf '%q|%q|%q|%5q\n' 'a b' '' foo x
__IN__
'a b'|''|foo|    x
__OUT__

test_oE -e 0 'printf with field width and precision from arguments'
printf '[%*d][%-*s][%.*s][%*d][%.*s]\n' 4 1 3 a 2 abc -3 5 -1 xyz
__IN__
[   1][a  ][ab][5  ][xyz]
__OUT__

test_oE -e 0 'printf with numbered field width'
printf '[%2$*1$s]\n' 4 a
__IN__
[   a]
__OUT__

test_oE -e 0 'printf %c and precision count bytes'
printf '%c\n' あ | od -An -tx1 | tr -s ' '
printf '%.1s\n' あ | od -An -tx1 | tr -s ' '
printf '[%4s]\n' あ
__IN__
 e3 0a
 e3 0a
[ あ]
__OUT__

test_oE -e 0 'printf %b pads the field after \c'
printf '[%-5b]tail' 'x\c'
echo '|'
printf '[%5b]tail' 'x\c'
echo '|'
__IN__
[x    |
[    x|
__OUT__

test_oE -e 0 'printf %b does not interpret octal without leading zero'
printf '%b\n' '\101\0101'
__IN__
\101A
__OUT__

test_oE -e 0 'printf -v assigns result to variable'
printf -v var '%s=%d\n' a 1 b 2
printf '[%s]\n' "$var"
__IN__
[a=1
b=2
]
__OUT__

test_oE -e 0 'printf -v does not print to standard output'
printf -v var foo
echo "$var"
__IN__
foo
__OUT__

test_oE -e 0 'printf -v with \c'
printf -v var '%b|%s' 'a\cb' c
echo "$var"
__IN__
a
__OUT__

test_O -d -e 1 'printf -v with read-only variable'
readonly var=old
printf -v var new
__IN__

test_O -d -e 2 'printf -v with invalid variable name'
printf -v a=b foo
__IN__

test_O -d -e 2 'printf without format'
printf
__IN__

test_O -d -e 2 'printf -v without format'
printf -v var
__IN__

test_oE -e 0 'printf --help'
printf --help | head -n 1
__IN__
printf: print formatted text
__OUT__

test_oE -e 0 'printf with format starting with hyphen'
printf -- '-%s\n' foo
__IN__
-foo
__OUT__

test_O -d -e 2 'printf with invalid option'
printf -x foo
__IN__

test_O -d -e 2 'printf with invalid conversion specifier'
printf 'foo%y'
__IN__

test_O -d -e 2 'printf with incomplete conversion specification'
printf '%5'
__IN__

test_O -d -e 2 'printf with length modifier'
printf '%ld' 1
__IN__

test_O -d -e 2 'printf with flag not allowed for conversion'
printf '%#d' 1
__IN__

test_O -d -e 2 'printf with decorated percent'
printf '%5%'
__IN__

test_O -d -e 2 'printf with zero position'
printf '%0$s' a
__IN__

test_oE -e 0 'printf with mixed numbered and unnumbered conversions'
printf '%2$s %s %s|' a b c d e f; echo
printf '[%*1$d]\n' 3 4
__IN__
b a b|d c d|f e f|
[  3]
[   4]
__OUT__

test_O -d -e 2 'printf with invalid Unicode escape'
printf '\uD800'
__IN__

test_o -d -e 1 'printf with missing numbered argument'
printf '%2$s|\n' a b c
__IN__
b|
|
__OUT__

test_o -d -e 1 'printf with invalid integer arguments'
printf '%d|%d|%d|%d\n' 5a '' "'45" 99999999999999999999
__IN__
5|0|52|9223372036854775807
__OUT__

test_o -d -e 1 'printf with too large precision from argument'
printf '[%.*s]\n' 99999999999 abc
__IN__
[abc]
__OUT__

test_o -d -e 1 'printf with invalid floating-point arguments'
printf '%g|%g|%g\n' 1x foo 1e999
__IN__
1|0|inf
__OUT__

test_o -d -e 1 'printf with floating-point underflow'
printf '%g\n' 1e-999
__IN__
0
__OUT__

test_oE -e 0 'printf %u with negative argument'
printf '%u|%x\n' -1 -1
__IN__
18446744073709551615|ffffffffffffffff
__OUT__

test_OE -e 0 'printf %c with empty or missing argument prints nothing'
printf '%c' ''
printf '%c'
__IN__

test_oE -e 0 'printf ignores arguments without conversions'
printf 'foo\n' a b
__IN__
foo
__OUT__

test_oE -e 0 'printf accepts POSIX format with portable option' -o portable
printf '%5.2s|%d|%b\n' abc 0x10 '\0101\c'
echo
__IN__
   ab|16|A
__OUT__

test_O -d -e 2 'printf rejects floating-point conversion with portable option' -o portable
printf '%f' 1
__IN__

test_O -d -e 2 'printf rejects %q with portable option' -o portable
printf '%q' 1
__IN__

test_O -d -e 2 'printf rejects asterisk with portable option' -o portable
printf '%*s' 1 a
__IN__

test_O -d -e 2 'printf rejects mixed numbering with portable option' -o portable
printf '%1$s %s' a b
__IN__

test_O -d -e 2 'printf rejects extended escape in format with portable option' -o portable
printf '\e'
__IN__

test_O -d -e 2 'printf rejects unknown escape in format with portable option' -o portable
printf '\q'
__IN__

test_O -d -e 2 'printf rejects non-POSIX escape in %b argument with portable option' -o portable
printf '%b' foo 'a\eb'
__IN__

test_O -d -e 2 'printf rejects precision for %c with portable option' -o portable
printf '%.1c' a
__IN__

test_O -d -e 2 'printf rejects octal escape larger than a byte with portable option' -o portable
printf '\400'
__IN__

test_O -d -e 2 'printf rejects large octal escape in %b argument with portable option' -o portable
printf '%b' '\0400'
__IN__

test_O -d -e 2 'printf rejects arguments for format without conversions with portable option' -o portable
printf 'foo' a
__IN__

test_O -d -e 2 'printf rejects empty argument for %c with portable option' -o portable
printf '%c' ''
__IN__

test_O -d -e 2 'printf rejects missing argument for %c with portable option' -o portable
printf '%c'
__IN__

test_O -d -e 2 'printf rejects \c after precision in %b argument with portable option' -o portable
printf '%.1b' 'ab\c'
__IN__

test_O -d -e 2 'printf rejects integer argument with leading blank with portable option' -o portable
printf '%d' ' 1'
__IN__

test_O -d -e 2 'printf rejects integer argument of lone quote with portable option' -o portable
printf '%d' "'"
__IN__

test_O -d -e 2 'printf rejects negative argument for unsigned conversion with portable option' -o portable
printf '%u' -1
__IN__

test_O -d -e 2 'printf rejects -v with portable option' -o portable
printf -v var foo
__IN__
