# printf-p.sh: test of the printf built-in for any POSIX-compliant shell

posix="true"

test_OE -e 0 'printf with empty format prints nothing'
printf ''
__IN__

test_oE -e 0 'printf prints normal characters as they are'
printf 'foo bar\n'
__IN__
foo bar
__OUT__

test_oE -e 0 'printf interprets escape sequences in format'
printf 'a\ab\bf\fr\rt\tv\v-\n' | od -An -c | tr -s ' '
__IN__
 a \a b \b f \f r \r t \t v \v - \n
__OUT__

test_oE -e 0 'printf interprets escaped backslash in format'
printf 'a\\b\n'
__IN__
a\b
__OUT__

test_oE -e 0 'printf interprets octal escape sequences in format'
printf '\101\102\60x\n'
__IN__
AB0x
__OUT__

test_oE -e 0 'printf prints percent sign'
printf '%%|%d%%%d\n' 1 2
__IN__
%|1%2
__OUT__

test_oE -e 0 'printf %d'
printf '%d,' 0 10 +345 -1278; echo
printf '%+d|% d|%5d|%-5d|%05d|%.3d\n' 1 2 3 4 5 6
printf '%d,' 011 0x1fE 0X1F \'a \"@; echo
__IN__
0,10,345,-1278,
+1| 2|    3|4    |00005|006
9,510,31,97,64,
__OUT__

test_oE -e 0 'printf %i'
printf '%i|%+i\n' -7 7
__IN__
-7|+7
__OUT__

test_oE -e 0 'printf %u, %o, %x, %X'
printf '%u|%o|%x|%X\n' 42 8 255 255
printf '%#o|%#x|%#X|%#x\n' 8 255 255 0
__IN__
42|10|ff|FF
010|0xff|0XFF|0
__OUT__

test_oE -e 0 'printf %c'
printf '[%c][%c][%3c][%-3c]\n' abc x y z
__IN__
[a][x][  y][z  ]
__OUT__

test_oE -e 0 'printf %s'
printf '[%s][%5s][%-5s][%.2s][%5.1s]\n' foo bar baz qux quux
__IN__
[foo][  bar][baz  ][qu][    q]
__OUT__

test_oE -e 0 'printf %s does not interpret escape sequences in argument'
printf '%s\n' 'a\tb%d'
__IN__
a\tb%d
__OUT__

test_oE -e 0 'printf %b interprets escape sequences in argument'
printf '%b\n' 'a\tb\\c\0101\n' | tr '\t' T
__IN__
aTb\cA

__OUT__

test_oE -e 0 'printf %b with field width and precision'
printf '[%5b][%-5.2b]\n' abc defg
__IN__
[  abc][de   ]
__OUT__

test_oE -e 0 'printf %b stops output at \c'
printf '%b|%s\n' 'foo\cbar' baz
echo
__IN__
foo
__OUT__

test_oE -e 0 'printf reuses format for remaining arguments'
printf '%s-%s\n' 1 2 3
__IN__
1-2
3-
__OUT__

test_oE -e 0 'printf uses empty string or zero for missing arguments'
printf '[%s][%d][%b]\n'
__IN__
[][0][]
__OUT__

test_oE -e 0 'printf with numbered conversions'
printf '%2$s %1$s|' a b c d; echo
printf '%3$s %1$s|' 1 2 3 4 5 6; echo
__IN__
b a|d c|
3 1|6 4|
__OUT__

test_oE -e 0 'printf with numbered conversion and percent'
printf '%1$d%%\n' 50
__IN__
50%
__OUT__

test_o -d -e n 'printf with argument not completely converted'
printf '%d|%d\n' 5a 2
__IN__
5|2
__OUT__

test_O -d -e n 'printf to closed standard output'
printf 'foo\n' >&-
__IN__
