export function set_hash(s) {
    history.replaceState(null, "", s);
}
export function get_hash() {
    return location.hash;
}