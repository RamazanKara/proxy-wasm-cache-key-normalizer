# proxy-wasm-cache-key-normalizer

Proxy-Wasm request filter that normalizes cache-relevant request metadata before
Varnish computes its cache hash.

It is designed for [vmod-wasm](https://github.com/RamazanKara/vmod-wasm), but
the module uses standard Proxy-Wasm request headers/properties where possible.

## What It Normalizes

- Lowercase `Host`
- Strip default `:80` and `:443` host ports
- Collapse duplicate path slashes
- Resolve `.` and `..` path segments
- Sort query parameters
- Remove tracking parameters such as `utm_*`, `gclid`, `fbclid`, and `msclkid`
- Emit a normalized cache-key hint header for VCL, logs, or observability

The module can run in `rewrite` mode or `report` mode. In `rewrite` mode it
updates `request.path` and `request.host`, which affects Varnish's default hash
inputs. In `report` mode it only emits headers.

## Varnish / vmod-wasm

```vcl
import wasm;

sub vcl_init {
    wasm.load("cachekey", "/etc/varnish/wasm/proxy_wasm_cache_key_normalizer.wasm");
    wasm.set_epoch_deadline(100);
    wasm.set_memory_limit(8388608);
}

sub vcl_recv {
    set req.http.X-Wasm-Action =
        wasm.proxy_wasm_on_request_configured("cachekey", "",
            {"{"mode":"rewrite","remove_query_prefixes":["utm_"],"remove_query_params":["gclid","fbclid"],"sort_query":true,"trailing_slash":"strip" }"});

    if (req.http.X-Wasm-Action != "0") {
        return (synth(500, "Cache key normalizer failed"));
    }
}
```

The default emitted headers are:

- `X-Cache-Key-Normalized: true|false`
- `X-Cache-Key: <method> <host><url>`

You can use the emitted key in custom VCL hash logic if desired:

```vcl
sub vcl_hash {
    if (req.http.X-Cache-Key) {
        hash_data(req.http.X-Cache-Key);
        return (lookup);
    }
}
```

## Configuration

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `mode` | string | `rewrite` | `rewrite` mutates request URL/Host; `report` only emits headers. |
| `lowercase_host` | bool | `true` | Lowercase `Host`. |
| `strip_default_port` | bool | `true` | Remove `:80` and `:443`. |
| `normalize_path` | bool | `true` | Collapse slashes and resolve dot segments. |
| `lowercase_path` | bool | `false` | Lowercase paths. Use only when origin paths are case-insensitive. |
| `sort_query` | bool | `true` | Sort query pairs by name and value. |
| `remove_query_params` | array | common click IDs | Exact query names to remove. |
| `remove_query_prefixes` | array | `["utm_"]` | Query name prefixes to remove. |
| `keep_query_params` | array | `[]` | Optional allowlist. If set, all other params are dropped. |
| `trailing_slash` | string | `preserve` | `preserve`, `strip`, or `add`. |
| `vary_headers` | array | `[]` | Headers to append to the emitted cache-key hint. |
| `cache_key_header` | string | `x-cache-key` | Header for the normalized cache-key hint. |
| `normalized_header` | string | `x-cache-key-normalized` | Header indicating whether URL/Host changed. |
| `emit_headers` | bool | `true` | Emit observability headers. |

## Build

```bash
cargo build --release --target wasm32-unknown-unknown
```

Artifact:

```text
target/wasm32-unknown-unknown/release/proxy_wasm_cache_key_normalizer.wasm
```

## Test

```bash
cargo fmt --all --check
cargo test --all
cargo clippy --target wasm32-unknown-unknown --all-targets -- -D warnings
cargo build --release --target wasm32-unknown-unknown
```

Integration test against a sibling `vmod-wasm` checkout:

```bash
VMOD_WASM_REPO=../vmod-wasm ./scripts/test-vmod-wasm.sh
```
