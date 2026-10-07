#!/usr/bin/env bash
# Builds a portable Windows folder (dist/annota) and zip from an MSYS2
# UCRT64 shell: annota.exe next to the GTK DLLs it links, plus the runtime
# data GTK looks up relative to the executable (icons, schemas, loaders).
#
# Usage (after `cargo build --release`): packaging/windows/bundle.sh
set -euo pipefail

prefix=$(cygpath -u "${MINGW_PREFIX:?run this from an MSYS2 UCRT64 shell}")
dist=dist/annota
pixbuf=lib/gdk-pixbuf-2.0/2.10.0

rm -rf dist
mkdir -p "$dist"

# Copies the MSYS2 DLLs a binary depends on (recursively, via ldd).
copy_deps() {
    ldd "$1" | awk -v bin="$prefix/bin/" 'index($3, bin) == 1 { print $3 }' |
        while read -r dll; do cp -u "$dll" "$dist/"; done
}

cp target/release/annota.exe README.md LICENSE "$dist/"
copy_deps "$dist/annota.exe"

# GApplication's single-instance check autolaunches a session bus helper.
cp "$prefix/bin/gdbus.exe" "$dist/"
copy_deps "$dist/gdbus.exe"

# Image loaders (SVG icons) with a cache using paths relative to the folder.
mkdir -p "$dist/$pixbuf/loaders"
cp "$prefix/$pixbuf/loaders/"*.dll "$dist/$pixbuf/loaders/"
for loader in "$dist/$pixbuf/loaders/"*.dll; do copy_deps "$loader"; done
gdk-pixbuf-query-loaders "$dist/$pixbuf/loaders/"*.dll |
    sed "s|$(cygpath -m "$PWD/$dist")/||g" >"$dist/$pixbuf/loaders.cache"

# GTK settings schemas (file and color dialogs).
mkdir -p "$dist/share/glib-2.0/schemas"
cp "$prefix/share/glib-2.0/schemas/"org.gtk.gtk4.*.gschema.xml "$dist/share/glib-2.0/schemas/"
glib-compile-schemas "$dist/share/glib-2.0/schemas"

# Icon themes used by the toolbar and GTK widgets.
mkdir -p "$dist/share/icons"
cp -r "$prefix/share/icons/Adwaita" "$prefix/share/icons/hicolor" "$dist/share/icons/"

(cd dist && zip -qr ../annota-windows-x86_64.zip annota)
du -sh "$dist" annota-windows-x86_64.zip
