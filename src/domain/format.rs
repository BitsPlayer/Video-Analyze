pub fn human_bytes(n: u64) -> String {
    const K: f64 = 1024.0;
    let x = n as f64;
    if x < K {
        format!("{n} B")
    } else if x < K * K {
        format!("{:.1} KiB", x / K)
    } else if x < K * K * K {
        format!("{:.1} MiB", x / (K * K))
    } else {
        format!("{:.2} GiB", x / (K * K * K))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_formats() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(2048), "2.0 KiB");
    }
}
