# umask-y.sh: yash-specific test of the umask built-in

test_oE -e 0 'long option name accepted as an extension'
umask 022
umask --symbolic
__IN__
u=rwx,g=rx,o=rx
__OUT__

test_O -d -e 2 'long option name rejected under the portable option' -o portable
umask --symbolic
__IN__

test_oE -e 0 'short option name still accepted under the portable option' -o portable
umask 022
umask -S
__IN__
u=rwx,g=rx,o=rx
__OUT__
