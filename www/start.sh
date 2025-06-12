set -e
set -x
export RUSTUP_TOOLCHAIN=nightly
wasm-pack build --out-dir www/pkg --target web --release --no-default-features --features "wasm-draw"
wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int -o pkg/kalc_plot_bg.wasm pkg/kalc_plot_bg.wasm
cp index index.html
sed -i "s/@@KALC@@/$(ls --color=never -d ./pkg/snippets/kalc-plot-*|sed 's@.*/@@')/" index.html
sed -i "s/@@RUPL@@/$(ls --color=never -d ./pkg/snippets/rupl-*|sed 's@.*/@@')/" index.html
if [ $# -ne 0 ]; then
    python3 -m http.server 8080
fi