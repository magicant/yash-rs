# declutil-y.sh: yash-specific test of declaration utilities

>tmpfile

test_oE 'pathname expansion and field splitting in printf A=$a'
a='1  tmp*  2'
printf "%s\n" A=$a
__IN__
A=1
tmpfile
2
__OUT__

test_oE 'tilde expansions in printf A=~:~'
HOME=/foo
printf "%s\n" A=~:~
__IN__
A=~:~
__OUT__

test_oE 'command command printf'
a='1  tmp*  2'
command command printf "%s\n" A=$a
__IN__
A=1
tmpfile
2
__OUT__
