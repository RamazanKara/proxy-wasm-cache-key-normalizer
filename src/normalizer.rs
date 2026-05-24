use crate::config::NormalizerConfig;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedRequest {
    pub url: String,
    pub host: String,
    pub cache_key: String,
    pub changed: bool,
}

pub fn normalize_request(
    method: &str,
    url: &str,
    host: &str,
    vary_headers: &[(String, String)],
    config: &NormalizerConfig,
) -> NormalizedRequest {
    let normalized_host = normalize_host(host, config);
    let normalized_url = normalize_url(url, config);
    let cache_key = build_cache_key(method, &normalized_host, &normalized_url, vary_headers);
    let changed = normalized_host != host || normalized_url != url;

    NormalizedRequest {
        url: normalized_url,
        host: normalized_host,
        cache_key,
        changed,
    }
}

pub fn normalize_host(host: &str, config: &NormalizerConfig) -> String {
    let mut normalized = host.trim().to_string();
    if config.lowercase_host {
        normalized = normalized.to_ascii_lowercase();
    }

    if config.strip_default_port {
        if let Some(without) = normalized.strip_suffix(":80") {
            normalized = without.to_string();
        } else if let Some(without) = normalized.strip_suffix(":443") {
            normalized = without.to_string();
        }
    }

    normalized
}

pub fn normalize_url(url: &str, config: &NormalizerConfig) -> String {
    let (path, query) = split_url(url);
    let mut path = if config.normalize_path {
        normalize_path(path)
    } else {
        path.to_string()
    };

    if config.lowercase_path {
        path = path.to_ascii_lowercase();
    }

    path = apply_trailing_slash(path, &config.trailing_slash);

    let query = normalize_query(query, config);
    if query.is_empty() {
        path
    } else {
        format!("{path}?{query}")
    }
}

pub fn normalize_path(path: &str) -> String {
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }

    let mut normalized = String::from("/");
    normalized.push_str(&parts.join("/"));
    normalized
}

pub fn normalize_query(query: &str, config: &NormalizerConfig) -> String {
    if query.is_empty() {
        return String::new();
    }

    let keep = config
        .keep_query_params
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    let remove = config
        .remove_query_params
        .iter()
        .map(|name| name.as_str())
        .collect::<Vec<_>>();
    let prefixes = config
        .remove_query_prefixes
        .iter()
        .map(|prefix| prefix.as_str())
        .collect::<Vec<_>>();

    let mut pairs = Vec::new();
    for raw in query.split('&') {
        if raw.is_empty() {
            continue;
        }
        let name = raw.split_once('=').map(|(name, _)| name).unwrap_or(raw);
        if !keep.is_empty() && !keep.contains(&name) {
            continue;
        }
        if remove.contains(&name) {
            continue;
        }
        if prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            continue;
        }
        pairs.push(raw.to_string());
    }

    if config.sort_query {
        pairs.sort_by(|a, b| {
            let (a_name, a_value) = split_query_pair(a);
            let (b_name, b_value) = split_query_pair(b);
            a_name.cmp(b_name).then_with(|| a_value.cmp(b_value))
        });
    }

    pairs.join("&")
}

fn split_url(url: &str) -> (&str, &str) {
    url.split_once('?').unwrap_or((url, ""))
}

fn split_query_pair(pair: &str) -> (&str, &str) {
    pair.split_once('=').unwrap_or((pair, ""))
}

fn apply_trailing_slash(mut path: String, mode: &str) -> String {
    match mode {
        "strip" if path.len() > 1 => {
            while path.len() > 1 && path.ends_with('/') {
                path.pop();
            }
            path
        }
        "add" if !path.ends_with('/') => {
            path.push('/');
            path
        }
        _ => path,
    }
}

fn build_cache_key(
    method: &str,
    host: &str,
    url: &str,
    vary_headers: &[(String, String)],
) -> String {
    let mut key = format!("{} {}{}", method.trim().to_ascii_uppercase(), host, url);
    for (name, value) in vary_headers {
        key.push('\n');
        key.push_str(&name.trim().to_ascii_lowercase());
        key.push(':');
        key.push_str(&normalize_header_value(value));
    }
    key
}

fn normalize_header_value(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_tracking_params_and_sorts_query() {
        let config = NormalizerConfig::default();
        let normalized = normalize_url("/item?utm_source=ad&b=2&a=1&gclid=x", &config);
        assert_eq!(normalized, "/item?a=1&b=2");
    }

    #[test]
    fn normalizes_path_segments() {
        let config = NormalizerConfig {
            trailing_slash: "strip".to_string(),
            ..NormalizerConfig::default()
        };
        assert_eq!(normalize_url("/a//b/../c/./?z=1", &config), "/a/c?z=1");
    }

    #[test]
    fn applies_keep_list_before_sorting() {
        let config = NormalizerConfig {
            keep_query_params: vec!["sku".to_string(), "page".to_string()],
            remove_query_params: Vec::new(),
            remove_query_prefixes: Vec::new(),
            ..NormalizerConfig::default()
        };
        assert_eq!(
            normalize_url("/search?debug=1&page=2&sku=abc", &config),
            "/search?page=2&sku=abc"
        );
    }

    #[test]
    fn lowercases_host_and_strips_default_port() {
        let config = NormalizerConfig::default();
        assert_eq!(normalize_host("EXAMPLE.TEST:80", &config), "example.test");
        assert_eq!(
            normalize_host("EXAMPLE.TEST:8443", &config),
            "example.test:8443"
        );
    }

    #[test]
    fn builds_cache_key_with_vary_headers() {
        let config = NormalizerConfig {
            vary_headers: vec!["accept-language".to_string()],
            ..NormalizerConfig::default()
        };
        let normalized = normalize_request(
            "get",
            "/item?b=2&a=1",
            "EXAMPLE.TEST:80",
            &[("accept-language".to_string(), " en-US,  en ".to_string())],
            &config,
        );
        assert_eq!(normalized.url, "/item?a=1&b=2");
        assert_eq!(
            normalized.cache_key,
            "GET example.test/item?a=1&b=2\naccept-language:en-US, en"
        );
    }
}
