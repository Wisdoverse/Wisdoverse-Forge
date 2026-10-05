#!/bin/bash
set -euo pipefail
unset DEB_BUILD_OPTIONS DEB_BUILD_PROFILES DH_OPTIONS
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get -y dist-upgrade
apt-get install -y --no-install-recommends build-essential dpkg-dev debhelper dh-exec dh-runit libaudit-dev libcrypt-dev libedit-dev libfido2-dev libpam0g-dev libselinux1-dev libssl-dev libwrap0-dev libwtmpdb-dev pkgconf zlib1g-dev ca-certificates curl gpgv devscripts git procps
cd /build
for file in openssh_10.5p1-1.dsc openssh_10.5p1.orig.tar.xz openssh_10.5p1-1.debian.tar.xz; do
    curl --fail --location --silent --show-error --retry 2 --connect-timeout 15 --max-time 120 "https://deb.debian.org/debian/pool/main/o/openssh/$file" -o "$file"
done
sha256sum -c <<'CHECKSUMS'
15a621f5d4c298127b57740e271ef571760686133f344a4841aa8e96af0ad532  openssh_10.5p1-1.dsc
9643e644c20dfef1271eaccc488ae4cebc9b5352b800c30771233880d985c74c  openssh_10.5p1.orig.tar.xz
4b19dea2fc78e93687d7f9e103542136d863e3d5c95e7d7be6088ac7979fb0cc  openssh_10.5p1-1.debian.tar.xz
CHECKSUMS
# Debian tag2upload signs this source. Do not install Sid runtime packages.
curl --fail --location --silent --show-error --retry 2 --connect-timeout 15 --max-time 120 https://deb.debian.org/debian/pool/main/d/debian-tag2upload-keyring/debian-tag2upload-keyring_1.2_all.deb -o tag2upload-keyring.deb
echo '6783d74cb547297c5dc231e89c2f3610d55f7628ba4b955fe33c41bc9556a6f8  tag2upload-keyring.deb' | sha256sum -c -
dpkg-deb -x tag2upload-keyring.deb tag2upload-keys
gpgv --status-fd 1 --keyring /build/tag2upload-keys/usr/share/keyrings/debian-tag2upload.pgp openssh_10.5p1-1.dsc | tee source-signature.status
# gpgv can return zero for expired keys. Do not accept that result without these signer status checks.
awk '$1 == "[GNUPG:]" {
    if ($2 == "GOODSIG" && $3 == "606D084E4683C079") good = 1
    if ($2 == "VALIDSIG" && $3 == "374D8CE4DB96E9CBD4C0972A606D084E4683C079") valid = 1
    if ($2 ~ /^(BADSIG|ERRSIG|NO_PUBKEY|EXPKEYSIG|KEYEXPIRED|EXPSIG|SIGEXPIRED|REVKEYSIG|KEYREVOKED|ERROR|FAILURE|NODATA)$/) bad = 1
} END {exit !(good && valid && !bad)}' source-signature.status
dpkg-source -x openssh_10.5p1-1.dsc
cd openssh-10.5p1
DEBFULLNAME='Forge Agent' DEBEMAIL=dev@example.com dch --no-conf --force-bad-version --no-auto-nmu --newversion '1:10.5p1-1~forge13+1' --distribution trixie 'Rebuild against Debian Trixie for OpenSSH client security fixes.'
dpkg-checkbuilddeps -Pnoudeb,pkg.openssh.nognome
dpkg-buildpackage -b -us -uc -j2 -Pnoudeb,pkg.openssh.nognome
cd /build
mkdir /out
cp openssh-client_10.5p1-1~forge13+1_*.deb openssh-common_10.5p1-1~forge13+1_*.deb /out/
for file in /out/*.deb; do
    test "$(dpkg-deb -f "$file" Version)" = '1:10.5p1-1~forge13+1'
    dpkg-deb -f "$file" Package Version Architecture Depends
    sha256sum "$file"
done
cp source-signature.status /out/
dpkg-query -W -f='${binary:Package} ${Version}\n' > /out/build-environment.txt
# The runtime stage does not contain build sources or build dependencies.
rm -rf /build/* /var/lib/apt/lists/*
