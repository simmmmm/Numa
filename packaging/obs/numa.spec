Name:           numa
Version:        @VERSION@
Release:        0
Summary:        RAW photo editor and library
License:        GPL-3.0-or-later
URL:            https://github.com/simmmmm/Numa
Source0:        numa-%{version}.tar.xz
ExclusiveArch:  x86_64
BuildRequires:  cargo
BuildRequires:  rust >= 1.92
BuildRequires:  gcc
BuildRequires:  hicolor-icon-theme
BuildRequires:  pkgconfig(gtk4) >= 4.14
BuildRequires:  pkgconfig(libadwaita-1) >= 1.5
# ort-sys builds a downloader that is never run here (ORT_LIB_PATH); it
# still links OpenSSL.
BuildRequires:  pkgconfig(openssl)
Requires:       hicolor-icon-theme
%if 0%{?suse_version}
Requires:       libvulkan1
%else
Requires:       vulkan-loader
%endif
# FLOW-013: tethering opens libgphoto2 when it is asked for, and goes without
# it; not linked, so nothing finds the dependency by itself.
%if 0%{?suse_version}
Recommends:     libgphoto2-6
%else
Recommends:     libgphoto2
%endif

# Numa's own copy of ONNX Runtime in /usr/lib/numa: not offered to anything
# else, and not taken from anywhere else.
%global __provides_exclude_from ^%{_prefix}/lib/numa/.*$
%global __requires_exclude ^libonnxruntime\\.so.*$
%global debug_package %{nil}

%description
Numa is a RAW photo editor and library. Add a folder, cull the shoot, develop
the frames you keep and export them. Your originals are never moved, copied or
written to.

%prep
%autosetup

%build
sh packaging/obs/build.sh build

%install
sh packaging/obs/build.sh install %{buildroot}

%files
%{_bindir}/numa
%{_prefix}/lib/numa/
%{_datadir}/numa/
%{_datadir}/applications/com.tijmen.Numa.desktop
%{_datadir}/metainfo/com.tijmen.Numa.metainfo.xml
%{_datadir}/icons/hicolor/*/apps/com.tijmen.Numa*.png
%{_datadir}/icons/hicolor/scalable/actions/*.svg
%{_datadir}/licenses/numa/
