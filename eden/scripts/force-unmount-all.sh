#!/bin/bash

ALL_EDENFS_MOUNTS=$(grep -e '^edenfs' /proc/mounts | awk '{print $2}')
for n in $ALL_EDENFS_MOUNTS; do
    # Sometimes, lazy unmounting causes other mounts to unmount.
    sudo umount -v -l "$n"
done

ALL_EDENFS_MOUNTS=$(grep -e '^edenfs' /proc/mounts | awk '{print $2}')
for n in $ALL_EDENFS_MOUNTS; do
    sudo umount -v -f "%f"
done
