use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct NormalizerConfig {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_true")]
    pub lowercase_host: bool,
    #[serde(default = "default_true")]
    pub strip_default_port: bool,
    #[serde(default = "default_true")]
    pub normalize_path: bool,
    #[serde(default)]
    pub lowercase_path: bool,
    #[serde(default = "default_true")]
    pub sort_query: bool,
    #[serde(default = "default_tracking_params")]
    pub remove_query_params: Vec<String>,
    #[serde(default = "default_tracking_prefixes")]
    pub remove_query_prefixes: Vec<String>,
    #[serde(default)]
    pub keep_query_params: Vec<String>,
    #[serde(default = "default_trailing_slash")]
    pub trailing_slash: String,
    #[serde(default)]
    pub vary_headers: Vec<String>,
    #[serde(default = "default_cache_key_header")]
    pub cache_key_header: String,
    #[serde(default = "default_normalized_header")]
    pub normalized_header: String,
    #[serde(default = "default_true")]
    pub emit_headers: bool,
}

impl Default for NormalizerConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            lowercase_host: true,
            strip_default_port: true,
            normalize_path: true,
            lowercase_path: false,
            sort_query: true,
            remove_query_params: default_tracking_params(),
            remove_query_prefixes: default_tracking_prefixes(),
            keep_query_params: Vec::new(),
            trailing_slash: default_trailing_slash(),
            vary_headers: Vec::new(),
            cache_key_header: default_cache_key_header(),
            normalized_header: default_normalized_header(),
            emit_headers: true,
        }
    }
}

impl NormalizerConfig {
    pub fn is_report_mode(&self) -> bool {
        self.mode.eq_ignore_ascii_case("report")
    }
}

pub fn parse_config(data: &[u8]) -> Result<NormalizerConfig, String> {
    if data.is_empty() {
        return Ok(NormalizerConfig::default());
    }

    let config: NormalizerConfig =
        serde_json_wasm::from_slice(data).map_err(|err| format!("invalid-config: {err}"))?;

    if config.mode != "rewrite" && config.mode != "report" {
        return Err("invalid-config: mode must be rewrite or report".to_string());
    }

    if !matches!(config.trailing_slash.as_str(), "preserve" | "strip" | "add") {
        return Err("invalid-config: trailing_slash must be preserve, strip, or add".to_string());
    }

    Ok(config)
}

fn default_mode() -> String {
    "rewrite".to_string()
}

fn default_true() -> bool {
    true
}

fn default_trailing_slash() -> String {
    "preserve".to_string()
}

fn default_cache_key_header() -> String {
    "x-cache-key".to_string()
}

fn default_normalized_header() -> String {
    "x-cache-key-normalized".to_string()
}

fn default_tracking_prefixes() -> Vec<String> {
    vec!["utm_".to_string()]
}

fn default_tracking_params() -> Vec<String> {
    vec![
        "fbclid".to_string(),
        "gclid".to_string(),
        "msclkid".to_string(),
        "yclid".to_string(),
        "mc_cid".to_string(),
        "mc_eid".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_strip_common_tracking_noise() {
        let config = NormalizerConfig::default();
        assert_eq!(config.mode, "rewrite");
        assert!(config.lowercase_host);
        assert!(config.sort_query);
        assert!(config.remove_query_prefixes.contains(&"utm_".to_string()));
        assert!(config.remove_query_params.contains(&"gclid".to_string()));
    }

    #[test]
    fn parses_custom_config() {
        let json = br#"{
            "mode":"report",
            "lowercase_path":true,
            "keep_query_params":["sku","page"],
            "remove_query_params":[],
            "remove_query_prefixes":[],
            "vary_headers":["accept-language"],
            "trailing_slash":"strip"
        }"#;
        let config = parse_config(json).unwrap();
        assert!(config.is_report_mode());
        assert!(config.lowercase_path);
        assert_eq!(config.keep_query_params, vec!["sku", "page"]);
        assert_eq!(config.vary_headers, vec!["accept-language"]);
        assert_eq!(config.trailing_slash, "strip");
    }

    #[test]
    fn rejects_unknown_mode() {
        let err = parse_config(br#"{"mode":"shadow"}"#).unwrap_err();
        assert!(err.contains("mode"));
    }
}
