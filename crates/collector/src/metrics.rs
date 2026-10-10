use std::collections::BTreeMap;

pub fn parse(text: &str) -> BTreeMap<String, f64> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            if name.starts_with('#') {
                return None;
            }
            let value: f64 = fields.next()?.parse().ok()?;
            value.is_finite().then(|| (name.to_owned(), value))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_real_halogen_fixture_and_optional_lines() {
        let values = parse(include_str!("../../../fixtures/halogen-0.17.3/metrics.txt"));
        assert_eq!(values["halogen:requests_total"], 1.0);
        assert_eq!(values["llamacpp:prompt_seconds_total"], 0.2677);
        assert!(!values.contains_key("halogen:admission_reserve_slots"));
        assert_eq!(parse("# ignored\nx 1e3\nbad nope\ny NaN\n")["x"], 1000.0);
    }
}
