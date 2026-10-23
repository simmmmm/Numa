FROM ubuntu:22.04

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
      libvulkan-dev glslang-tools spirv-tools cmake \
    && rm -rf /var/lib/apt/lists/*

RUN pip3 install --no-cache-dir "meson>=1.6" ninja packaging

COPY stack.sh /opt/tools/stack.sh
RUN chmod +x /opt/tools/stack.sh
RUN /opt/tools/stack.sh wayland
RUN /opt/tools/stack.sh wayland-protocols
RUN /opt/tools/stack.sh glib
RUN /opt/tools/stack.sh cairo
RUN /opt/tools/stack.sh harfbuzz
RUN /opt/tools/stack.sh pango

RUN /opt/tools/stack.sh glslc

COPY stack-top.sh /opt/tools/stack-top.sh
RUN chmod +x /opt/tools/stack-top.sh
RUN /opt/tools/stack-top.sh vulkan-headers
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
 && cd rt && git sparse-checkout init --cone && git sparse-checkout set rtdata/dcpprofiles

RUN apt-get update && apt-get install -y --no-install-recommends libssl-dev \
      hicolor-icon-theme \
    && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /opt/adwaita && curl -sSfL \
      https://download.gnome.org/sources/adwaita-icon-theme/46/adwaita-icon-theme-46.0.tar.xz \
    | tar -xJ -C /opt/adwaita --strip-components=1

RUN mkdir -p /opt/onnxruntime \
 && curl -sSfL https://github.com/microsoft/onnxruntime/releases/download/v1.28.0/onnxruntime-linux-x64-1.28.0.tgz -o /tmp/ort.tgz \
 && echo "a3e1b79d7bb1bf09696ce675f49e4064e6c81f6202b8225624fff0e93f8d6407  /tmp/ort.tgz" | sha256sum -c \
 && tar -xzf /tmp/ort.tgz --strip-components=1 -C /opt/onnxruntime \
 && rm /tmp/ort.tgz

RUN apt-get update && apt-get install -y --no-install-recommends libsoup-3.0-dev \
      libjson-glib-dev libprotobuf-c-dev libsqlite3-dev gperf \
    && rm -rf /var/lib/apt/lists/*
RUN curl -sSfL https://download.gnome.org/sources/libshumate/1.7/libshumate-1.7.0.tar.xz -o /tmp/shumate.tar.xz \
 && echo "3896cc4b6a7bf83141c27df23ce48cdd68467603bc6b764960307cdcbbef80e5  /tmp/shumate.tar.xz" | sha256sum -c \
 && mkdir /tmp/shumate && tar -xJf /tmp/shumate.tar.xz -C /tmp/shumate --strip-components=1 \
 && meson setup /tmp/shumate/_build /tmp/shumate --prefix=/usr/local --libdir=lib/x86_64-linux-gnu \
      --buildtype=release -Dgir=false -Dvapi=false -Dgtk_doc=false -Ddemos=false -Dsysprof=disabled \
 && meson compile -C /tmp/shumate/_build && meson install -C /tmp/shumate/_build && ldconfig \
 && rm -rf /tmp/shumate /tmp/shumate.tar.xz

WORKDIR /src
