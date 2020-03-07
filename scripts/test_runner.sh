#! /usr/bin/bash -x
sleep 1
sleep 5 &
sleep 10 &
sleep 2
pstree -p $$
echo 'parent exit'
wait
exit 0
