FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential pkg-config curl ca-certificates file desktop-file-utils \
      librsvg2-common libgdk-pixbuf2.0-bin libglib2.0-bin \
      patchelf python3 python3-pip python3-venv zsync libimage-exiftool-perl git \
      gettext libffi-dev libpcre2-dev libmount-dev libselinux1-dev zlib1g-dev \
      libexpat1-dev libxml2-utils libpixman-1-dev libfreetype-dev \
      libfontconfig1-dev libpng-dev libxcb1-dev libxcb-render0-dev \
      libxcb-shm0-dev libx11-dev libxext-dev libxrender-dev libxi-dev \
      libxrandr-dev libxcursor-dev libxdamage-dev libxinerama-dev libxkbcommon-dev \
      libwayland-dev wayland-protocols libegl1-mesa-dev libgles2-mesa-dev \
      libepoxy-dev libgraphene-1.0-dev libfribidi-dev libdrm-dev libgbm-dev \
      libjpeg-dev libtiff-dev libgdk-pixbuf-2.0-dev libappstream-dev \
      libvulkan-dev glslang-tools spirv-tools \
    && rm -rf /var/lib/apt/lists/*

RUN pip3 install --break-system-packages --no-cache-dir "meson>=1.6" ninja

COPY stack.sh /opt/tools/stack.sh
RUN chmod +x /opt/tools/stack.sh
RUN /opt/tools/stack.sh wayland
RUN /opt/tools/stack.sh glib
RUN /opt/tools/stack.sh cairo
RUN /opt/tools/stack.sh harfbuzz
RUN /opt/tools/stack.sh pango

RUN apt-get update && apt-get install -y --no-install-recommends glslc \
    && rm -rf /var/lib/apt/lists/*

COPY stack-top.sh /opt/tools/stack-top.sh
RUN chmod +x /opt/tools/stack-top.sh
RUN /opt/tools/stack-top.sh gtk
RUN /opt/tools/stack-top.sh libadwaita

ENV PKG_CONFIG_PATH="/usr/local/lib/x86_64-linux-gnu/pkgconfig:/usr/local/lib/pkgconfig:/usr/local/share/pkgconfig"
ENV LD_LIBRARY_PATH="/usr/local/lib/x86_64-linux-gnu:/usr/local/lib"
ENV XDG_DATA_DIRS="/usr/local/share:/usr/share"

RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /opt/tools
RUN curl -sSLO https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage \
 && curl -sSLo linuxdeploy-plugin-gtk.sh https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh \
 && curl -sSLO https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage \
 && chmod +x ./*.AppImage ./*.sh

WORKDIR /opt/profiles
RUN git clone --depth 1 --branch 5.11 --filter=blob:none --sparse \
      https://github.com/Beep6581/RawTherapee.git rt \
 && cd rt && git sparse-checkout set rtdata/dcpprofiles

RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev \
      adwaita-icon-theme hicolor-icon-theme \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
