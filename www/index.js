import init from "./pkg/kalc_plot.js";

async function run() {
    await init(); // this will call #[wasm_bindgen(start)] automatically
}

run();
