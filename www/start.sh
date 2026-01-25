set -e
set -x
export RUSTUP_TOOLCHAIN=nightly
wasm-pack build --out-dir www/pkg --target web --release --no-default-features --features "wasm-draw,kalc-lib"
ls -l pkg/kalc_plot_bg.wasm
wasm-opt -O4 -all -o pkg/kalc_plot_bg.wasm pkg/kalc_plot_bg.wasm
ls -l pkg/kalc_plot_bg.wasm
cp index index.html
sed -i "s/@@RUPL@@/$(ls --color=never -d ./pkg/snippets/rupl-*|sed 's@.*/@@')/" index.html
if [ $# -ne 0 ]; then
    python3 -m http.server 8080
fi
