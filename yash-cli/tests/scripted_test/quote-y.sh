# quote-y.sh: yash-specific test of quoting

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
