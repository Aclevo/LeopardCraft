#!/usr/bin/env bash
# Build the Linux download formats (.deb, .rpm, AppImage) from an already built
# `leopardcraft` binary.
#
#   packaging/linux/package.sh <version> <path-to-binary> <output-dir>
#
# The script is self-contained apart from the host tools listed below, and it
# never touches the source tree: everything is staged in a temporary directory.
#
# Requires: dpkg-deb, rpmbuild, curl, mksquashfs (squashfs-tools) and, for the
# AppImage, appimagetool (downloaded on demand).
set -euo pipefail

version="${1:?usage: package.sh <version> <binary> <output-dir>}"
binary="${2:?usage: package.sh <version> <binary> <output-dir>}"
outdir="${3:?usage: package.sh <version> <binary> <output-dir>}"

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${here}/../.." && pwd)"
name="leopardcraft-${version}-linux-x86_64"
icon="${here}/leopardcraft.png"
desktop="${here}/leopardcraft.desktop"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

mkdir -p "${outdir}"
chmod +x "${binary}"

stage_tree() {
    local root="$1"
    mkdir -p "${root}/usr/bin" \
        "${root}/usr/share/applications" \
        "${root}/usr/share/icons/hicolor/256x256/apps" \
        "${root}/usr/share/doc/leopardcraft"
    cp "${binary}" "${root}/usr/bin/leopardcraft"
    cp "${desktop}" "${root}/usr/share/applications/leopardcraft.desktop"
    cp "${icon}" "${root}/usr/share/icons/hicolor/256x256/apps/leopardcraft.png"
    cp "${repo_root}/README.md" "${repo_root}/LICENSE-MIT" "${repo_root}/LICENSE-APACHE" \
        "${root}/usr/share/doc/leopardcraft/"
}

# --- Debian package -------------------------------------------------------
debroot="${work}/deb"
stage_tree "${debroot}"
mkdir -p "${debroot}/DEBIAN"
cat > "${debroot}/DEBIAN/control" <<EOF
Package: leopardcraft
Version: ${version}
Section: graphics
Priority: optional
Architecture: amd64
Installed-Size: $(du -sk "${debroot}/usr" | cut -f1)
Depends: libc6, libxkbcommon0, libx11-6, libxrandr2, libxi6, libxcursor1, libwayland-client0
Recommends: libvulkan1
Maintainer: LeopardCraft contributors
Homepage: https://github.com/Aclevo/LeopardCraft
Description: Clean-room 3D modelling, rendering and animation studio
 LeopardCraft is an offline 3D modelling, rendering and animation
 application written in pure Rust, with a node material graph, mesh
 editing tools and a CPU path tracer.
EOF
dpkg-deb --root-owner-group --build "${debroot}" "${outdir}/${name}.deb"
echo "built ${outdir}/${name}.deb"

# --- RPM package ----------------------------------------------------------
topdir="${work}/rpm"
mkdir -p "${topdir}"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
rpmroot="${work}/rpmroot"
stage_tree "${rpmroot}"
cat > "${topdir}/SPECS/leopardcraft.spec" <<EOF
Name:           leopardcraft
Version:        ${version}
Release:        1
Summary:        Clean-room 3D modelling, rendering and animation studio
License:        MIT OR Apache-2.0
URL:            https://github.com/Aclevo/LeopardCraft
Requires:       libX11 libxkbcommon libwayland-client libXcursor libXi
Recommends:     vulkan-loader
AutoReqProv:    no

%description
LeopardCraft is an offline 3D modelling, rendering and animation application
written in pure Rust, with a node material graph, mesh editing tools and a
CPU path tracer.

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a ${rpmroot}/. %{buildroot}/

%files
/usr/bin/leopardcraft
/usr/share/applications/leopardcraft.desktop
/usr/share/icons/hicolor/256x256/apps/leopardcraft.png
/usr/share/doc/leopardcraft
EOF
rpmbuild --define "_topdir ${topdir}" --define "debug_package %{nil}" -bb \
    "${topdir}/SPECS/leopardcraft.spec" > /dev/null
cp "${topdir}"/RPMS/*/*.rpm "${outdir}/${name}.rpm"
echo "built ${outdir}/${name}.rpm"

# --- AppImage -------------------------------------------------------------
appdir="${work}/LeopardCraft.AppDir"
stage_tree "${appdir}"
# AppImage expects the desktop entry and icon at the AppDir root as well.
cp "${desktop}" "${appdir}/leopardcraft.desktop"
cp "${icon}" "${appdir}/leopardcraft.png"
cat > "${appdir}/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "${HERE}/usr/bin/leopardcraft" "$@"
EOF
chmod +x "${appdir}/AppRun"

tool="${work}/appimagetool"
if command -v appimagetool > /dev/null 2>&1; then
    tool="$(command -v appimagetool)"
else
    curl -fsSL -o "${tool}" \
        "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
    chmod +x "${tool}"
fi
# APPIMAGE_EXTRACT_AND_RUN keeps this working on hosts and CI images without FUSE.
VERSION="${version}" ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 \
    "${tool}" "${appdir}" "${outdir}/${name}.AppImage" > /dev/null
echo "built ${outdir}/${name}.AppImage"

# --- Verify the packages with their own tooling ---------------------------
dpkg-deb --info "${outdir}/${name}.deb" > /dev/null
if ! dpkg-deb --contents "${outdir}/${name}.deb" | grep -q 'usr/bin/leopardcraft$'; then
    echo "error: ${name}.deb is missing usr/bin/leopardcraft" >&2
    exit 1
fi

rpm -qip "${outdir}/${name}.rpm" > /dev/null
if ! rpm -qlp "${outdir}/${name}.rpm" | grep -qx '/usr/bin/leopardcraft'; then
    echo "error: ${name}.rpm is missing /usr/bin/leopardcraft" >&2
    exit 1
fi
rpm -qlp "${outdir}/${name}.rpm"

chmod +x "${outdir}/${name}.AppImage"
APPIMAGE_EXTRACT_AND_RUN=1 "${outdir}/${name}.AppImage" --version

echo "verified ${name}.deb, ${name}.rpm and ${name}.AppImage"
