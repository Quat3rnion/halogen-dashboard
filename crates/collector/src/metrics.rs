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
    fn parses_0181_with_labelled_build_info_without_losing_cache_metrics() {
        let values = parse(include_str!("../../../fixtures/halogen-0.18.1/metrics.txt"));
        assert_eq!(values["halogen:cache_bytes"], 1047506868.0);
        assert_eq!(values["halogen:cache_misses_total"], 7.0);
        assert_eq!(values["halogen:cache_stats_age_seconds"], 1.999);
        assert_eq!(
            values["halogen:build_info{version=\"0.18.1\",engine_version=\"0.18.1\",model=\"halogen-v2-abliterated\"}"],
            1.0
        );
    }
    #[test]
    fn parses_real_halogen_fixture_and_optional_lines() {
        let values = parse(include_str!("../../../fixtures/halogen-0.17.3/metrics.txt"));
        assert_eq!(values["halogen:requests_total"], 1.0);
        assert_eq!(values["llamacpp:prompt_seconds_total"], 0.2677);
        assert!(!values.contains_key("halogen:admission_reserve_slots"));
        assert_eq!(parse("# ignored\nx 1e3\nbad nope\ny NaN\n")["x"], 1000.0);
    }
}
