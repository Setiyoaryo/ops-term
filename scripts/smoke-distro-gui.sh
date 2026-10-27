#!/bin/sh
# Run only inside a disposable container with public source/binary mounts.
set -eu
family=$1
binary=$2
case "$family" in
    cachyos)
        pacman -Syu --noconfirm --needed xorg-server-xvfb mesa libxkbcommon-x11 libx11 libxrandr libxi libxcursor libxinerama libglvnd ttf-dejavu xdotool imagemagick openssh weston python
        ;;
    ubuntu)
        export DEBIAN_FRONTEND=noninteractive
        apt-get update -qq
        apt-get install -y -qq xvfb mesa-utils libgl1-mesa-dri libegl1 libxkbcommon-x11-0 libx11-6 libxrandr2 libxi6 libxcursor1 libxinerama1 fonts-dejavu-core xdotool imagemagick openssh-client weston python3
        ;;
    *) exit 2 ;;
esac
export LIBGL_ALWAYS_SOFTWARE=1
export PYTHONDONTWRITEBYTECODE=1
cd /opt/source
xvfb-run -a python3 scripts/smoke-egui-gns3.py "$binary" /tmp/gns3-gui
python3 scripts/smoke-egui-wayland.py "$binary"
