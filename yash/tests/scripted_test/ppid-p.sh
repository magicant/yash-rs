# ppid-p.tst: test of the $PPID variable for any POSIX-compliant shell

posix="true"

test_OE -e 0 'PPID is parent process ID'
echo $PPID >variable.out
echo $(ps -o ppid= $$) >ps.out
diff variable.out ps.out
__IN__

test_OE -e 0 'PPID does not change in subshell'
echo $PPID >main.out
(echo $PPID) >subshell.out
diff main.out subshell.out
__IN__
