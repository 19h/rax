fn main() {
    let status = 0x8000_000D;
    let observed = rax::user::windows::nt::status_to_error(status);
    println!("status={status:#010x} observed-dos-error={observed} expected-native=299");
    assert_eq!(observed, 299);
}
