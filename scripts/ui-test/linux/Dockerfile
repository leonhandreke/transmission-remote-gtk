# The Linux UI test environment for transmission-remote-gtk: a headless
# GNOME Shell (Mutter) started the way gnome-shell's own tests start it (see
# session.sh), AT-SPI, transmission-daemon, and what's needed to build the
# app with cargo. It tracks the latest Fedora, for the latest GNOME and GTK.
#
# scripts/ui-test/uitest linux up builds the image and starts the container;
# it mounts the repo at /src and runs session.sh from there.
FROM fedora:latest

RUN dnf -y install --setopt=install_weak_deps=False \
        gcc pkgconf-pkg-config rust cargo \
        gtk4-devel libsoup3-devel glib2-devel gettext-devel blueprint-compiler \
        glib-networking \
        gnome-shell mutter mutter-tests gnome-settings-daemon gsettings-desktop-schemas \
        python3-dbusmock pipewire wireplumber \
        dbus-daemon dbus-tools at-spi2-core python3 python3-gobject \
        transmission-daemon transmission-cli \
        mesa-dri-drivers adwaita-icon-theme adwaita-sans-fonts adwaita-mono-fonts \
        google-noto-sans-fonts google-noto-emoji-fonts dejavu-sans-fonts \
        procps-ng psmisc findutils which file \
    && dnf clean all

# The session runs as a normal user, as gnome-shell expects
RUN useradd -m -u 1000 tester \
    && mkdir -p /run/user/1000 /home/tester/.cargo /home/tester/target \
    && chown tester /run/user/1000 /home/tester/.cargo /home/tester/target \
    && chmod 700 /run/user/1000

USER tester
ENV XDG_RUNTIME_DIR=/run/user/1000 \
    TRG_SESSION_ENV=/run/user/1000/trg-session.env \
    TRG_IN_CONTAINER=1 \
    CARGO_HOME=/home/tester/.cargo \
    CARGO_TARGET_DIR=/home/tester/target \
    LANG=C.UTF-8 \
    GSK_RENDERER=cairo
WORKDIR /src
CMD ["/src/scripts/ui-test/linux/session.sh"]
