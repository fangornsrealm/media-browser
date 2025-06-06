name := 'media-browser'
export APPID := 'eu.fangornsrealm.MediaBrowser'

rootdir := ''
prefix := '/usr'

base-dir := absolute_path(clean(rootdir / prefix))

export INSTALL_DIR := base-dir / 'share'

cargo-target-dir := env('CARGO_TARGET_DIR', 'target')
bin-src := cargo-target-dir / 'release' / name
bin-dst := base-dir / 'bin' / name

desktop := APPID + '.desktop'
desktop-src := 'res' / desktop
desktop-dst := clean(rootdir / prefix) / 'share' / 'applications' / desktop

metainfo := APPID + '.metainfo.xml'
metainfo-src := 'res' / metainfo
metainfo-dst := clean(rootdir / prefix) / 'share' / 'metainfo' / metainfo

icons-src := 'res' / 'icons' / 'hicolor'
icons-dst := clean(rootdir / prefix) / 'share' / 'icons' / 'hicolor'

# Default recipe which runs `just build-release`
default: build-release

# Runs `cargo clean`
clean:
    cargo clean

# Removes vendored dependencies
clean-vendor:
    rm -rf .cargo vendor vendor.tar

# `cargo clean` and removes vendored dependencies
clean-dist: clean clean-vendor

# Compiles with debug profile
build-debug *args:
    cargo build {{args}}

# Compiles with release profile
build-release *args: (build-debug '--release' args)

# Compiles release profile with vendored dependencies
build-vendored *args: vendor-extract (build-release '--frozen --offline' args)

# Runs a clippy check
check *args:
    cargo clippy --all-features {{args}} -- -W clippy::pedantic

# Runs a clippy check with JSON message format
check-json: (check '--message-format=json')

# Developer target
dev *args:
    cargo fmt
    just run {{args}}

# Run with debug logs
run *args:
    cargo build --release
    env RUST_LOG=media_browser=info RUST_BACKTRACE=full {{bin-src}} {{args}}

# Run tests
test *args:
    cargo test {{args}}

# Installs files
install:
    install -Dm0755 {{bin-src}} {{bin-dst}}
    install -Dm0644 {{desktop-src}} {{desktop-dst}}
    install -Dm0644 {{metainfo-src}} {{metainfo-dst}}
    for size in `ls {{icons-src}}`; do \
        install -Dm0644 "{{icons-src}}/$size/apps/{{APPID}}.svg" "{{icons-dst}}/$size/apps/{{APPID}}.svg"; \
    done

# Uninstalls installed files
uninstall:
    rm -f {{bin-dst}}

# Installs files locally
[no-cd]
install-local:
    install -Dm0755 {{bin-src}} ~/.local/bin/{{bin-dst}}
    install -Dm0644 {{desktop-src}} ${XDG_DATA_HOME:-~/.local/share}/{{desktop-dst}}
    install -Dm0644 {{metainfo-src}} ${XDG_DATA_HOME:-~/.local/share}/{{metainfo-dst}}
    install -Dm0644 {{icons-src}} ${XDG_DATA_HOME:-~/.local/share}/{{icons-dst}}

# Uninstalls locally installed files
uninstall-local:
    rm ~/.local/bin/{{bin-dst}}
    rm ${XDG_DATA_HOME:-~/.local/share}/{{desktop-dst}}
    rm ${XDG_DATA_HOME:-~/.local/share}/{{metainfo-dst}}
    rm ${XDG_DATA_HOME:-~/.local/share}/{{icons-dst}}

# Compiles and packages deb with release profile
build-deb:
    command -v cargo-deb || cargo install cargo-deb
    cargo deb

[no-cd]
install-deb:
    apt install --reinstall ./target/debian/*.deb

# Compiles and packages rpm with release profile
[no-cd]
build-rpm: build-release
    command -v cargo-generate-rpm || cargo install cargo-generate-rpm
    strip -s {{bin-src}}
    cargo generate-rpm

[no-cd]
install-rpm:
    dnf install ./target/generate-rpm/*.rpm

# Vendor dependencies locally
vendor:
    #!/usr/bin/env bash
    mkdir -p .cargo
    cargo vendor --sync Cargo.toml | head -n -1 > .cargo/config.toml
    echo 'directory = "vendor"' >> .cargo/config.toml
    echo >> .cargo/config.toml
    echo '[env]' >> .cargo/config.toml
    if [ -n "${SOURCE_DATE_EPOCH}" ]
    then
        source_date="$(date -d "@${SOURCE_DATE_EPOCH}" "+%Y-%m-%d")"
        echo "VERGEN_GIT_COMMIT_DATE = \"${source_date}\"" >> .cargo/config.toml
    fi
    if [ -n "${SOURCE_GIT_HASH}" ]
    then
        echo "VERGEN_GIT_SHA = \"${SOURCE_GIT_HASH}\"" >> .cargo/config.toml
    fi
    tar pcf vendor.tar .cargo vendor
    rm -rf .cargo vendor

# Extracts vendored dependencies
vendor-extract:
    rm -rf vendor
    tar pxf vendor.tar
