#!/bin/bash

# Generates a blank FAT16 FS
# Needs to be run as root

set -e

IMG_PATH=/tmp/img.fat

dd if=/dev/zero of=$IMG_PATH bs=512 count=20480

echo "label: dos
label-id: 0xd3553a1
device: $IMG_PATH
unit: sectors

start=        2048, size=       18432, type=6" | /sbin/sfdisk $IMG_PATH

LOOPBACK=$(losetup -f)
TMP_DIR=$(mktemp -d)

losetup -o 1048576 $LOOPBACK $IMG_PATH

mkfs.vfat -F 16 $LOOPBACK

mount -t vfat $LOOPBACK $TMP_DIR
mkdir $TMP_DIR/{bin,cfg,data}
umount $LOOPBACK

echo $IMG_PATH
