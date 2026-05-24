mod config;
mod normalizer;

use config::{parse_config, NormalizerConfig};
use normalizer::normalize_request;
use proxy_wasm::traits::*;
use proxy_wasm::types::*;
use std::rc::Rc;

const MODULE_VERSION: &str = "0.1.0";

proxy_wasm::main! {{
    proxy_wasm::set_log_level(LogLevel::Info);
    proxy_wasm::set_root_context(|_| -> Box<dyn RootContext> {
        Box::new(CacheKeyRoot {
            state: Rc::new(ConfigState::default()),
            metrics: None,
        })
    });
}}

#[derive(Clone, Default)]
struct ConfigState {
    config: NormalizerConfig,
    error: Option<String>,
}

#[derive(Clone, Default)]
struct MetricIds {
    requests_total: Option<u32>,
    rewrites_total: Option<u32>,
    reports_total: Option<u32>,
    config_errors_total: Option<u32>,
}

struct CacheKeyRoot {
    state: Rc<ConfigState>,
    metrics: Option<MetricIds>,
}

impl Context for CacheKeyRoot {}

impl RootContext for CacheKeyRoot {
    fn on_vm_start(&mut self, _vm_configuration_size: usize) -> bool {
        proxy_wasm::hostcalls::log(
            LogLevel::Info,
            &format!("proxy-wasm-cache-key-normalizer v{MODULE_VERSION} starting"),
        )
        .ok();

        self.metrics = Some(MetricIds {
            requests_total: define_counter("cache_key_normalizer_requests_total"),
            rewrites_total: define_counter("cache_key_normalizer_rewrites_total"),
            reports_total: define_counter("cache_key_normalizer_reports_total"),
            config_errors_total: define_counter("cache_key_normalizer_config_errors_total"),
        });

        true
    }

    fn on_configure(&mut self, _plugin_configuration_size: usize) -> bool {
        let bytes = self.get_plugin_configuration().unwrap_or_default();
        let state = match parse_config(&bytes) {
            Ok(config) => ConfigState {
                config,
                error: None,
            },
            Err(error) => {
                proxy_wasm::hostcalls::log(
                    LogLevel::Error,
                    &format!("cache-key-normalizer: {error}"),
                )
                .ok();
                ConfigState {
                    config: NormalizerConfig::default(),
                    error: Some(error),
                }
            }
        };

        self.state = Rc::new(state);
        true
    }

    fn get_type(&self) -> Option<ContextType> {
        Some(ContextType::HttpContext)
    }

    fn create_http_context(&self, _context_id: u32) -> Option<Box<dyn HttpContext>> {
        Some(Box::new(CacheKeyFilter {
            state: Rc::clone(&self.state),
            metrics: self.metrics.clone().unwrap_or_default(),
        }))
    }
}

struct CacheKeyFilter {
    state: Rc<ConfigState>,
    metrics: MetricIds,
}

impl Context for CacheKeyFilter {}

impl HttpContext for CacheKeyFilter {
    fn on_http_request_headers(&mut self, _num_headers: usize, _end_of_stream: bool) -> Action {
        increment(self.metrics.requests_total);

        if let Some(error) = self.state.error.as_deref() {
            increment(self.metrics.config_errors_total);
            self.set_http_request_header("x-cache-key-normalizer-error", Some(error));
            return Action::Continue;
        }

        let method =
            property_string(self, vec!["request", "method"]).unwrap_or_else(|| "GET".to_string());
        let url = property_string(self, vec!["request", "path"]).unwrap_or_else(|| "/".to_string());
        let host = self
            .get_http_request_header("host")
            .or_else(|| property_string(self, vec!["request", "host"]))
            .unwrap_or_default();

        let vary_headers = collect_vary_headers(self, &self.state.config);
        let normalized = normalize_request(&method, &url, &host, &vary_headers, &self.state.config);

        if self.state.config.emit_headers {
            self.set_http_request_header(
                &self.state.config.normalized_header,
                Some(if normalized.changed { "true" } else { "false" }),
            );
            self.set_http_request_header(
                &self.state.config.cache_key_header,
                Some(&normalized.cache_key),
            );
        }

        if self.state.config.is_report_mode() {
            increment(self.metrics.reports_total);
            return Action::Continue;
        }

        if normalized.changed {
            increment(self.metrics.rewrites_total);
            self.set_property(vec!["request", "path"], Some(normalized.url.as_bytes()));
            if !normalized.host.is_empty() {
                self.set_property(vec!["request", "host"], Some(normalized.host.as_bytes()));
            }
        }

        Action::Continue
    }
}

fn collect_vary_headers<C: HttpContext>(
    ctx: &C,
    config: &NormalizerConfig,
) -> Vec<(String, String)> {
    config
        .vary_headers
        .iter()
        .filter_map(|name| {
            ctx.get_http_request_header(name)
                .map(|value| (name.clone(), value))
        })
        .collect()
}

fn property_string<C: Context>(ctx: &C, path: Vec<&str>) -> Option<String> {
    ctx.get_property(path)
        .and_then(|bytes| String::from_utf8(bytes).ok())
}

fn define_counter(name: &str) -> Option<u32> {
    proxy_wasm::hostcalls::define_metric(MetricType::Counter, name).ok()
}

fn increment(metric_id: Option<u32>) {
    if let Some(metric_id) = metric_id {
        proxy_wasm::hostcalls::increment_metric(metric_id, 1).ok();
    }
}
