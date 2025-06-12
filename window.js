const canvas = document.getElementById('canvas');
const ctx = canvas.getContext("2d", {desynchronized: true, alpha: false});
export function draw(slice, width) {
    const clamped = new Uint8ClampedArray(slice);
    const height = clamped.length / (width * 4);
    const image = new ImageData(clamped, width, height);
    ctx.putImageData(image, 0, 0);
}
export function get_canvas() {
    return canvas;
}
export function resize(x, y) {
    canvas.width = x;
    canvas.height = y;
}
export function dpr() {
    return window.devicePixelRatio
}
