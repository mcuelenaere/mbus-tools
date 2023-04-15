pub fn calculate_checksum<'a>(bytes: impl IntoIterator<Item=&'a u8>) -> u8 {
    let mut sum: u8 = 0;
    for b in bytes.into_iter() {
        sum = sum.wrapping_add(*b);
    }
    sum
}