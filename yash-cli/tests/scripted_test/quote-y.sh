# quote-y.sh: yash-specific test of quoting

(
setup -d

posix="true"

# POSIX does not imply that quote removal should be applied before the expanded
# word is assigned to the unset/empty variable. However, existing shells seem
# to perform quote removal, so yash follows them. Also note that the resultant
# value of the parameter expansion has quote removal already applied, so it is
# subject to field splitting.
# TODO The first expected line should not contain a space
test_oE 'quotes in substitution of expansion ${a=b}'
bracket ${a=\ \!\$x\%\&\(\)\*\+\,\-\.\/ \# \"x\" \'x\'}
bracket ${b=\0\1\2\3\4\5\6\7\8\9\:\;\<\=\>\? \\ \\\\}
bracket ${c=\@\A\B\C\D\E\F\G\H\I\J\K\L\M\N\O\P\Q\R\S\T\U\V\W\X\Y\Z\[\]\^\_}
bracket ${d=\a\b\c\d\e\f\g\h\i\j\k\l\m\n\o\p\q\r\s\t\u\v\w\x\y\z\{\|\}\~ \`\`}
bracket ${e=a"b"c} ${f=a"*"c} ${g=a"\"\""c} ${h=a"\\"c} ${i=a"''"c}
bracket ${j=a'b'c} ${k=a'*'c} ${l=a'""'c}   ${m=a'\'c}
bracket $a
bracket $b
bracket $c
bracket $d
bracket $e $f $g $h $i
bracket $j $k $l $m
__IN__
[ !$x%&()*+,-./][#]["x"]['x']
[0123456789:;<=>?][\][\\]
[@ABCDEFGHIJKLMNOPQRSTUVWXYZ[]^_]
[abcdefghijklmnopqrstuvwxyz{|}~][``]
[abc][a*c][a""c][a\c][a''c]
[abc][a*c][a""c][a\c]
[!$x%&()*+,-./][#]["x"]['x']
[0123456789:;<=>?][\][\\]
[@ABCDEFGHIJKLMNOPQRSTUVWXYZ[]^_]
[abcdefghijklmnopqrstuvwxyz{|}~][``]
[abc][a*c][a""c][a\c][a''c]
[abc][a*c][a""c][a\c]
__OUT__

# \{ and \} are tested below
test_oE 'quotes in substitution of expansion ${a=b} in double quotes'
bracket "${a=\ \!\$x\%\&\(\)\*\+\,\-\.\/ \# \"x\" \'x\'}"
bracket "${b=\0\1\2\3\4\5\6\7\8\9\:\;\<\=\>\? \\ \\\\}"
bracket "${c=\@\A\B\C\D\E\F\G\H\I\J\K\L\M\N\O\P\Q\R\S\T\U\V\W\X\Y\Z\[\]\^\_}"
bracket "${d=\a\b\c\d\e\f\g\h\i\j\k\l\m\n\o\p\q\r\s\t\u\v\w\x\y\z\|\~ \`\`}"
bracket "${e=a"b"c}" "${f=a"*"c}" "${g=a"\"\""c}" "${h=a"\\"c}" "${i=a"''"c}"
bracket "${j=a'b'c}" "${k=a'*'c}" "${l=a'""'c}"   "${m=a'\'c}"
bracket "$a"
bracket "$b"
bracket "$c"
bracket "$d"
bracket "$e" "$f" "$g" "$h" "$i"
bracket "$j" "$k" "$l" "$m"
__IN__
[\ \!$x\%\&\(\)\*\+\,\-\.\/ \# "x" \'x\']
[\0\1\2\3\4\5\6\7\8\9\:\;\<\=\>\? \ \\]
[\@\A\B\C\D\E\F\G\H\I\J\K\L\M\N\O\P\Q\R\S\T\U\V\W\X\Y\Z\[\]\^\_]
[\a\b\c\d\e\f\g\h\i\j\k\l\m\n\o\p\q\r\s\t\u\v\w\x\y\z\|\~ ``]
[abc][a*c][a""c][a\c][a''c]
[a'b'c][a'*'c][a''c][a'\'c]
[\ \!$x\%\&\(\)\*\+\,\-\.\/ \# "x" \'x\']
[\0\1\2\3\4\5\6\7\8\9\:\;\<\=\>\? \ \\]
[\@\A\B\C\D\E\F\G\H\I\J\K\L\M\N\O\P\Q\R\S\T\U\V\W\X\Y\Z\[\]\^\_]
[\a\b\c\d\e\f\g\h\i\j\k\l\m\n\o\p\q\r\s\t\u\v\w\x\y\z\|\~ ``]
[abc][a*c][a""c][a\c][a''c]
[a'b'c][a'*'c][a''c][a'\'c]
__OUT__

# TODO This should be moved to quote-p.sh
test_o -d -e 2 '\{ and \} in substitution of expansions in double quotes'
a=a
bracket "${a+1\{2\}3}" "${u-1\{2\}3}" "${b=1\{2\}3}"
bracket "$b"
eval '"${u?1\{2\}3}"'
__IN__
[1\{2}3][1\{2}3][1\{2}3]
[1\{2}3]
__OUT__

)

test_oE 'null character in dollar-single-quotes'
printf '%s\n' a$'b\0c'd w$'x\x0y'z 1$'2\c@3'4
__IN__

__OUT__

test_O -d -e 2 'too large octal escape in dollar-single-quotes'
printf '%s\n' $'\777'
__IN__

test_oE 'no dollar-single-quotes inside double quotes'
null=
printf '%s\n' "$'\x20$null'"
__IN__
$'\x20'
__OUT__

# This behavior is different from yash 2.
test_oE 'backslash preceding EOF is left intact'
"$TESTEE" -c 'printf "[%s]\n" 123\'
__IN__
[123\]
__OUT__

: TODO function definition not yet implemented <<'__OUT__'
test_oE 'line continuation in function definition'
\
f\
u\
n\
c\
t\
i\
o\
\
n\
	\
f"u\
n"c \
(\
 )\
\
{ echo foo; }
func
__IN__
foo
__OUT__

test_oE 'line continuation in parameter expansion'
f=foo
# echo ${#?}
echo \
$\
{\
\
#\
\
?\
\
}
__IN__
1
__OUT__
: TODO nested parameter expansion not yet implemented <<'__OUT__'
test_oE 'line continuation in parameter expansion'
f=foo
# echo ${#?} ${${f}} ${f[1,2]:+x}
echo \
$\
{\
\
#\
\
?\
\
} $\
\
{\
\
$\
\
{\
\
f\
\
}\
\
} $\
{\
f\
\
[\
\
1\
\
,\
\
2\
\
]\
\
:\
\
+\
\
x\
\
}
__IN__
1 foo x
__OUT__

test_O -d -e 2 'unclosed single quotation'
echo 'foo
-
__IN__
#'

test_O -d -e 2 'unclosed double quotation (direct)'
echo "foo
__IN__
#"

test_O -d -e 2 'unclosed double quotation (in parameter expansion)'
echo ${foo-"bar}
__IN__
#"
