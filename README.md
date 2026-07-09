# zenix-macros

Proc-macro crate for [zenix](https://github.com/gachors/zenix).

Provides the `#[openapi]` attribute macro that annotates async handler functions
with OpenAPI metadata (path, method, tags, description, etc.) and generates
registration glue for the zenix server.

```rust
#[zenix::openapi(
    path = "/hello",
    method = "get",
    tag = "Greetings",
    description = "Returns a friendly greeting"
)]
async fn hello() -> Value {
    json!({ "message": "Hello, world!" })
}
```
